//! ASIO audio engine for device management and stream handling
//!
//! Provides high-level interface for:
//! - Enumerating ASIO devices
//! - Opening input/output streams
//! - Managing audio callbacks
//!
//! ## Latency Measurement
//!
//! Uses frame-based burst detection for accurate latency measurement:
//! - Channel 0: 10ms burst every 100ms (10Hz update rate)
//! - Channel 1: Frame counter (sawtooth 0.0-1.0) for loss detection
//!
//! Latency is calculated using sample frame counters shared between
//! input and output callbacks, providing sample-accurate timing.
//! This eliminates the artificial delays caused by ring buffer accumulation.

use crate::audio::analyzer::Analyzer;
use crate::audio::burst::{BurstEvent, BurstGenerator, DetectionEvent};
use crate::audio::detector::BurstDetector;
use crate::audio::latency::{LatencyAnalyzer, LatencyResult};
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleRate, Stream, StreamConfig};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

// Re-export crossbeam for lock-free audio callback channels
use crossbeam_channel;

/// Ring buffer size in samples (enough for ~0.5 second at 96kHz)
const RING_BUFFER_SIZE: usize = 65536;

/// Errors that can occur during audio engine operations
#[derive(Error, Debug)]
pub enum AudioEngineError {
    #[error("No ASIO devices found")]
    NoDevicesFound,

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Failed to open stream: {0}")]
    StreamError(String),

    #[error("Sample rate mismatch: expected {expected}, got {actual}")]
    SampleRateMismatch { expected: u32, actual: u32 },

    #[error("ASIO host not available")]
    AsioNotAvailable,

    #[error("No input channels available")]
    NoInputChannels,

    #[error("No output channels available")]
    NoOutputChannels,
}

/// Audio device information
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device name
    pub name: String,
    /// Whether this is the default device
    pub is_default: bool,
    /// Supported sample rates
    pub sample_rates: Vec<u32>,
    /// Number of input channels
    pub input_channels: u16,
    /// Number of output channels
    pub output_channels: u16,
}

/// Audio engine state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    /// Engine is stopped
    Stopped,
    /// Engine is running and processing audio
    Running,
    /// Engine encountered an error
    Error,
}

/// Connection state for auto-reconnection tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connected and operating normally
    Connected,
    /// Disconnected, attempting to reconnect
    Reconnecting {
        /// Current reconnect attempt number (1-based)
        attempt: u32,
    },
    /// Reconnection failed after max attempts, requires manual intervention
    Failed,
}

/// Analysis results from comparing sent and received signals
///
/// Compatible with previous MLS-based interface for backward compatibility.
#[derive(Debug, Clone, Default)]
pub struct AnalysisResult {
    /// Measured latency in samples
    pub latency_samples: usize,
    /// Measured latency in milliseconds
    pub latency_ms: f64,
    /// Correlation confidence (0.0 to 1.0)
    pub confidence: f32,
    /// Number of lost samples detected
    pub lost_samples: usize,
    /// Number of corrupted samples detected
    pub corrupted_samples: usize,
    /// Whether the signal is healthy
    pub is_healthy: bool,
}

impl From<LatencyResult> for AnalysisResult {
    fn from(lr: LatencyResult) -> Self {
        Self {
            latency_samples: lr.latency_samples,
            latency_ms: lr.latency_ms,
            confidence: lr.confidence,
            lost_samples: 0,
            corrupted_samples: 0,
            is_healthy: lr.confidence > 0.5,
        }
    }
}

/// Shared state between audio callbacks and main thread
///
/// Only contains items accessed from the main thread via `analyze()`.
/// BurstGenerator and BurstDetector are moved directly into their respective
/// closures to eliminate all Mutex contention in the audio callback hot path.
struct SharedState {
    /// Latency analyzer for frame-based measurement (main thread only)
    latency_analyzer: Mutex<LatencyAnalyzer>,
    /// Frame-based loss detector for counter channel (main thread only)
    frame_analyzer: Mutex<Analyzer>,
    /// Latest analysis result (main thread only)
    last_result: Mutex<Option<AnalysisResult>>,
}

/// ASIO audio engine for managing audio streams
pub struct AudioEngine {
    state: EngineState,
    sample_rate: u32,
    device_name: Option<String>,
    host: Option<Host>,
    device: Option<Device>,
    input_stream: Option<Stream>,
    output_stream: Option<Stream>,
    shared_state: Option<Arc<SharedState>>,
    /// Consumer for counter samples (frame counter channel for loss detection)
    counter_consumer: Option<ringbuf::HeapCons<f32>>,
    /// Receiver for burst events from output callback (lock-free crossbeam)
    burst_event_rx: Option<crossbeam_channel::Receiver<BurstEvent>>,
    /// Receiver for detection events from input callback (lock-free crossbeam)
    detection_event_rx: Option<crossbeam_channel::Receiver<DetectionEvent>>,
    /// Running flag (shared with callbacks via Arc)
    running: Option<Arc<AtomicBool>>,
    /// Output sample counter (shared with output callback via Arc)
    output_samples: Option<Arc<std::sync::atomic::AtomicUsize>>,
    /// Input sample counter (shared with input callback via Arc)
    input_samples: Option<Arc<std::sync::atomic::AtomicUsize>>,
    /// Output frame counter (shared with output callback via Arc)
    output_frame_counter: Option<Arc<AtomicU64>>,
    /// Input frame counter (shared with input callback via Arc)
    input_frame_counter: Option<Arc<AtomicU64>>,
    /// Pre-allocated buffer for counter sample reads
    counter_buffer: Vec<f32>,
}

impl AudioEngine {
    /// Create a new audio engine with default settings
    pub fn new() -> Self {
        Self {
            state: EngineState::Stopped,
            sample_rate: crate::DEFAULT_SAMPLE_RATE,
            device_name: None,
            host: None,
            device: None,
            input_stream: None,
            output_stream: None,
            shared_state: None,
            counter_consumer: None,
            burst_event_rx: None,
            detection_event_rx: None,
            running: None,
            output_samples: None,
            input_samples: None,
            output_frame_counter: None,
            input_frame_counter: None,
            counter_buffer: Vec::new(),
        }
    }

    /// Get current engine state
    pub fn state(&self) -> EngineState {
        self.state
    }

    /// Get configured sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Set sample rate (must be called before start)
    pub fn set_sample_rate(&mut self, rate: u32) {
        if (8000..=384000).contains(&rate) {
            self.sample_rate = rate;
        }
    }

    /// Get the ASIO host
    fn get_asio_host() -> Result<Host> {
        #[cfg(target_os = "windows")]
        {
            cpal::host_from_id(cpal::HostId::Asio)
                .map_err(|e| anyhow!("Failed to get ASIO host: {}", e))
        }

        #[cfg(not(target_os = "windows"))]
        {
            // On non-Windows, return default host for testing
            Ok(cpal::default_host())
        }
    }

    /// List available ASIO devices
    ///
    /// # Returns
    /// Vector of device information for all available ASIO devices
    pub fn list_devices() -> Result<Vec<DeviceInfo>> {
        let host = Self::get_asio_host()?;
        let mut devices = Vec::new();

        let default_input = host.default_input_device().map(|d| d.name().ok());
        let default_output = host.default_output_device().map(|d| d.name().ok());

        for device in host.devices()? {
            let name = device.name().unwrap_or_else(|_| "Unknown".to_string());

            let is_default = default_input
                .as_ref()
                .map(|d| d.as_ref() == Some(&name))
                .unwrap_or(false)
                || default_output
                    .as_ref()
                    .map(|d| d.as_ref() == Some(&name))
                    .unwrap_or(false);

            // Get supported configs
            let input_channels = device
                .default_input_config()
                .map(|c| c.channels())
                .unwrap_or(0);

            let output_channels = device
                .default_output_config()
                .map(|c| c.channels())
                .unwrap_or(0);

            // Common sample rates to check
            let common_rates = [44100, 48000, 88200, 96000, 176400, 192000];
            let mut sample_rates = Vec::new();

            if let Ok(configs) = device.supported_output_configs() {
                for config in configs {
                    for &rate in &common_rates {
                        if (config.min_sample_rate().0..=config.max_sample_rate().0).contains(&rate)
                            && !sample_rates.contains(&rate)
                        {
                            sample_rates.push(rate);
                        }
                    }
                }
            }

            sample_rates.sort();

            devices.push(DeviceInfo {
                name,
                is_default,
                sample_rates,
                input_channels,
                output_channels,
            });
        }

        Ok(devices)
    }

    /// Select an ASIO device by name
    ///
    /// # Arguments
    /// * `name` - Name of the ASIO device to use
    pub fn select_device(&mut self, name: &str) -> Result<()> {
        let host = Self::get_asio_host()?;

        let device = host
            .devices()?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| AudioEngineError::DeviceNotFound(name.to_string()))?;

        self.host = Some(host);
        self.device = Some(device);
        self.device_name = Some(name.to_string());

        Ok(())
    }

    /// Get the selected device name
    pub fn device_name(&self) -> Option<&str> {
        self.device_name.as_deref()
    }

    /// Start audio processing
    ///
    /// Opens input and output streams on the selected device and begins
    /// generating burst signals and analyzing received audio for latency.
    pub fn start(&mut self) -> Result<()> {
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| anyhow!("No device selected"))?;

        // Get device's default output config
        let default_output = device.default_output_config();
        let default_input = device.default_input_config();

        tracing::info!(
            "Device default output config: {:?}",
            default_output
                .as_ref()
                .map(|c| (c.sample_rate().0, c.channels()))
        );
        tracing::info!(
            "Device default input config: {:?}",
            default_input
                .as_ref()
                .map(|c| (c.sample_rate().0, c.channels()))
        );

        // Use configured sample rate, with fallback to device default
        let device_rate = default_output
            .as_ref()
            .map(|c| c.sample_rate().0)
            .unwrap_or(self.sample_rate);
        let actual_sample_rate = self.sample_rate;
        tracing::info!("Using configured sample rate: {} Hz", actual_sample_rate);
        if device_rate != actual_sample_rate {
            tracing::info!(
                "Device default rate: {} Hz (will fallback if configured rate fails)",
                device_rate
            );
        }

        // Get device channel counts
        let output_channels = default_output.as_ref().map(|c| c.channels()).unwrap_or(2);
        let input_channels = default_input.as_ref().map(|c| c.channels()).unwrap_or(2);

        tracing::info!(
            "Device channel count: {} output, {} input",
            output_channels,
            input_channels
        );

        // Try configured rate first, fall back to device default if it fails
        let rates_to_try = if device_rate != actual_sample_rate {
            vec![actual_sample_rate, device_rate]
        } else {
            vec![actual_sample_rate]
        };

        let mut effective_rate = actual_sample_rate;
        let mut output_config = StreamConfig {
            channels: output_channels,
            sample_rate: SampleRate(actual_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };
        let mut input_config = StreamConfig {
            channels: input_channels,
            sample_rate: SampleRate(actual_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Test which sample rate works by trying a dummy build
        for &rate in &rates_to_try {
            output_config.sample_rate = SampleRate(rate);
            input_config.sample_rate = SampleRate(rate);
            match device.build_output_stream(
                &output_config,
                |_: &mut [f32], _: &cpal::OutputCallbackInfo| {},
                |_| {},
                None,
            ) {
                Ok(_stream) => {
                    effective_rate = rate;
                    if rate != actual_sample_rate {
                        tracing::warn!(
                            "Configured rate {} Hz failed, using device default {} Hz",
                            actual_sample_rate,
                            rate
                        );
                    }
                    break;
                }
                Err(e) => {
                    tracing::warn!("Sample rate {} Hz failed: {}", rate, e);
                    continue;
                }
            }
        }

        // Update configs with the effective rate
        output_config.sample_rate = SampleRate(effective_rate);
        input_config.sample_rate = SampleRate(effective_rate);
        tracing::info!("Effective sample rate: {} Hz", effective_rate);

        // Counter ring buffer: ch1 samples for loss detection only
        // NOTE: Burst samples are NOT buffered - detection happens inline in callback
        let counter_ring = HeapRb::<f32>::new(RING_BUFFER_SIZE);
        let (mut counter_producer, counter_consumer) = counter_ring.split();

        // Lock-free crossbeam channels for burst/detection events
        let (burst_event_tx, burst_event_rx) = crossbeam_channel::bounded::<BurstEvent>(32);
        let (detection_event_tx, detection_event_rx) =
            crossbeam_channel::bounded::<DetectionEvent>(32);

        // BurstGenerator and BurstDetector are moved directly into closures (no Mutex)
        let mut burst_gen = BurstGenerator::new(effective_rate);
        let mut burst_detector = BurstDetector::new(effective_rate);

        // Main-thread-only analyzers
        let latency_analyzer = LatencyAnalyzer::new(effective_rate);
        let frame_analyzer = Analyzer::new(&[], effective_rate);

        let shared_state = Arc::new(SharedState {
            latency_analyzer: Mutex::new(latency_analyzer),
            frame_analyzer: Mutex::new(frame_analyzer),
            last_result: Mutex::new(None),
        });

        // Standalone atomics shared with callbacks via Arc (no SharedState contention)
        let running = Arc::new(AtomicBool::new(true));
        let output_frame_counter = Arc::new(AtomicU64::new(0));
        let input_frame_counter = Arc::new(AtomicU64::new(0));
        let output_samples = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let input_samples = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Create output stream - BurstGenerator moved into closure (lock-free)
        let output_running = Arc::clone(&running);
        let output_counter = Arc::clone(&output_frame_counter);
        let output_sample_count = Arc::clone(&output_samples);
        let num_output_channels = output_channels as usize;
        let output_stream = device.build_output_stream(
            &output_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if output_running.load(Ordering::Relaxed) {
                    let start_counter = output_counter.load(Ordering::Acquire);
                    let mut frame_count = 0usize;

                    for (i, frame) in data.chunks_mut(num_output_channels).enumerate() {
                        // Channel 0: Burst signal (generator owned by this closure)
                        let (sample, is_burst_start) = burst_gen.next_sample();
                        if !frame.is_empty() {
                            frame[0] = sample;
                        }

                        // Send burst event via lock-free crossbeam channel
                        if is_burst_start {
                            let _ = burst_event_tx.try_send(BurstEvent {
                                start_frame: start_counter + i as u64,
                            });
                        }

                        // Channel 1: Frame counter as normalized sawtooth (0.0 to 1.0)
                        if frame.len() > 1 {
                            let counter = (start_counter + i as u64) & 0xFFFF;
                            frame[1] = (counter as f32) / 65536.0;
                        }

                        // Fill remaining channels with silence
                        for ch in frame.iter_mut().skip(2) {
                            *ch = 0.0;
                        }
                        frame_count += 1;
                    }

                    output_counter.fetch_add(frame_count as u64, Ordering::Release);

                    let prev = output_sample_count.fetch_add(frame_count, Ordering::Relaxed);
                    if prev == 0 {
                        tracing::info!(
                            "Output callback started: {} frames ({} channels), burst mode, ch0={:.4}, ch1={:.4}",
                            frame_count,
                            num_output_channels,
                            data.first().copied().unwrap_or(0.0),
                            data.get(1).copied().unwrap_or(0.0)
                        );
                    }
                } else {
                    data.fill(0.0);
                }
            },
            move |err| {
                tracing::error!("Output stream error: {}", err);
            },
            None,
        )?;

        // Create input stream - BurstDetector and counter producer moved into closure (lock-free)
        let input_running = Arc::clone(&running);
        let input_counter = Arc::clone(&input_frame_counter);
        let input_sample_count = Arc::clone(&input_samples);
        let num_input_channels = input_channels as usize;

        let input_stream = device.build_input_stream(
            &input_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if input_running.load(Ordering::Relaxed) {
                    let frame_count = data.len() / num_input_channels;
                    let input_frame_start = input_counter.load(Ordering::Acquire);

                    // Inline burst detection (detector owned by this closure, no Mutex)
                    for (i, frame) in data.chunks(num_input_channels).enumerate() {
                        if !frame.is_empty() {
                            let sample = frame[0];
                            let current_frame = input_frame_start + i as u64;

                            if burst_detector.process(sample, i).is_some() {
                                let _ = detection_event_tx.try_send(DetectionEvent {
                                    input_frame: current_frame,
                                });
                            }
                        }

                        // Counter ring buffer for loss detection (producer owned, no Mutex)
                        if frame.len() > 1 {
                            let _ = counter_producer.try_push(frame[1]);
                        }
                    }

                    input_counter.fetch_add(frame_count as u64, Ordering::Release);

                    let prev = input_sample_count.fetch_add(frame_count, Ordering::Relaxed);
                    if prev == 0 {
                        let max_level_ch0 = data
                            .chunks(num_input_channels)
                            .filter_map(|f| f.first())
                            .map(|x| x.abs())
                            .fold(0.0f32, f32::max);
                        let max_level_ch1 = data
                            .chunks(num_input_channels)
                            .filter_map(|f| f.get(1))
                            .map(|x| x.abs())
                            .fold(0.0f32, f32::max);
                        tracing::info!(
                            "Input callback started: {} frames ({} channels), ch0 max: {:.4}, ch1 max: {:.4}",
                            frame_count,
                            num_input_channels,
                            max_level_ch0,
                            max_level_ch1
                        );
                    }
                }
            },
            move |err| {
                tracing::error!("Input stream error: {}", err);
            },
            None,
        )?;

        // Start streams
        output_stream.play()?;
        input_stream.play()?;

        // Store everything
        self.output_stream = Some(output_stream);
        self.input_stream = Some(input_stream);
        self.shared_state = Some(shared_state);
        self.counter_consumer = Some(counter_consumer);
        self.burst_event_rx = Some(burst_event_rx);
        self.detection_event_rx = Some(detection_event_rx);
        self.running = Some(running);
        self.output_samples = Some(output_samples);
        self.input_samples = Some(input_samples);
        self.output_frame_counter = Some(output_frame_counter);
        self.input_frame_counter = Some(input_frame_counter);
        self.counter_buffer = vec![0.0f32; RING_BUFFER_SIZE / 2];
        self.state = EngineState::Running;
        self.sample_rate = effective_rate;

        tracing::info!(
            "Audio engine started (burst mode): {} @ {}Hz, 10Hz latency updates",
            self.device_name.as_deref().unwrap_or("unknown"),
            effective_rate
        );

        Ok(())
    }

    /// Stop audio processing
    pub fn stop(&mut self) -> Result<()> {
        if let Some(ref running) = self.running {
            running.store(false, Ordering::Relaxed);
        }

        self.input_stream = None;
        self.output_stream = None;
        self.shared_state = None;
        self.counter_consumer = None;
        self.burst_event_rx = None;
        self.detection_event_rx = None;
        self.running = None;
        self.output_samples = None;
        self.input_samples = None;
        self.output_frame_counter = None;
        self.input_frame_counter = None;
        self.counter_buffer = Vec::new();

        self.state = EngineState::Stopped;

        tracing::info!("Audio engine stopped");

        Ok(())
    }

    /// Analyze and match burst detections with bursts
    ///
    /// Call this periodically from the main thread to:
    /// 1. Register burst events from output callback
    /// 2. Match detection events from input callback using frame arithmetic
    /// 3. Process counter samples for loss detection
    ///
    /// This uses frame-based timing instead of wall-clock timestamps,
    /// eliminating the ~500ms error caused by ring buffer accumulation.
    ///
    /// # Returns
    /// Analysis result if a detection was matched with a burst
    pub fn analyze(&mut self) -> Option<AnalysisResult> {
        let counter_consumer = self.counter_consumer.as_mut()?;
        let shared_state = self.shared_state.as_ref()?;
        let burst_event_rx = self.burst_event_rx.as_ref()?;
        let detection_event_rx = self.detection_event_rx.as_ref()?;

        // Register any pending burst events from output callback
        if let Ok(mut latency_analyzer) = shared_state.latency_analyzer.lock() {
            while let Ok(event) = burst_event_rx.try_recv() {
                latency_analyzer.register_burst(event);
            }
        }

        // Process detection events from input callback using frame-based matching
        let mut result = AnalysisResult::default();
        let mut had_detection = false;

        if let Ok(mut latency_analyzer) = shared_state.latency_analyzer.lock() {
            while let Ok(detection) = detection_event_rx.try_recv() {
                // Frame-based matching - simple arithmetic, no timestamps!
                if let Some(latency_result) = latency_analyzer.match_detection(&detection) {
                    result = latency_result.into();
                    result.is_healthy = result.confidence > 0.5;
                    had_detection = true;
                }
            }

            // If no new detection, use last known result with time-based confidence decay.
            // When input goes silent (e.g. VBMatrix route muted), burst detections stop.
            // Confidence must decay to 0 so signal_lost triggers within ~1 second.
            if !had_detection {
                if let Some(last) = latency_analyzer.last_result() {
                    result.latency_samples = last.latency_samples;
                    result.latency_ms = last.latency_ms;
                    // Time-based decay: half-life of 0.3 seconds
                    // ~0.5s: confidence ≈ 0.31, ~0.6s: confidence ≈ 0.25 (below 0.3 threshold)
                    let elapsed = last.timestamp.elapsed().as_secs_f32();
                    result.confidence = last.confidence * 0.5f32.powf(elapsed / 0.3);
                    result.is_healthy = result.confidence > 0.3;
                }
            }
        }

        // Frame-based loss detection from counter channel (pre-allocated buffer)
        let counter_available = counter_consumer.occupied_len();
        if counter_available > 0 {
            let read_count = counter_available.min(self.counter_buffer.len());
            let counter_read = counter_consumer.pop_slice(&mut self.counter_buffer[..read_count]);
            let counter_samples = &self.counter_buffer[..counter_read];

            if let Ok(mut frame_analyzer) = shared_state.frame_analyzer.lock() {
                let frame_loss = frame_analyzer.detect_frame_loss(counter_samples);
                result.lost_samples = frame_loss;
                if frame_loss > 0 {
                    result.is_healthy = false;
                }
            }
        }

        // Store result
        if let Ok(mut last) = shared_state.last_result.lock() {
            *last = Some(result.clone());
        }

        Some(result)
    }

    /// Get the last analysis result
    pub fn last_result(&self) -> Option<AnalysisResult> {
        self.shared_state
            .as_ref()
            .and_then(|s| s.last_result.lock().ok())
            .and_then(|r| r.clone())
    }

    /// Get sample counts for debugging (output, input)
    pub fn sample_counts(&self) -> (usize, usize) {
        let out = self
            .output_samples
            .as_ref()
            .map(|s| s.load(Ordering::Relaxed))
            .unwrap_or(0);
        let inp = self
            .input_samples
            .as_ref()
            .map(|s| s.load(Ordering::Relaxed))
            .unwrap_or(0);
        (out, inp)
    }

    /// Get latency measurement update rate in Hz
    pub fn update_rate(&self) -> f32 {
        // Burst-based system runs at 10Hz (100ms cycles)
        10.0
    }

    /// Get average latency from analyzer
    pub fn average_latency_ms(&self) -> Option<f64> {
        self.shared_state.as_ref().and_then(|s| {
            s.latency_analyzer
                .lock()
                .ok()
                .map(|a| a.average_latency_ms())
        })
    }

    /// Get measurement count from analyzer
    pub fn measurement_count(&self) -> u64 {
        self.shared_state
            .as_ref()
            .and_then(|s| {
                s.latency_analyzer
                    .lock()
                    .ok()
                    .map(|a| a.measurement_count())
            })
            .unwrap_or(0)
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_engine_creation() {
        let engine = AudioEngine::new();
        assert_eq!(engine.state(), EngineState::Stopped);
        assert_eq!(engine.sample_rate(), crate::DEFAULT_SAMPLE_RATE);
    }

    #[test]
    fn test_engine_default() {
        let engine = AudioEngine::default();
        assert_eq!(engine.state(), EngineState::Stopped);
    }

    #[test]
    fn test_update_rate() {
        let engine = AudioEngine::new();
        assert!((engine.update_rate() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_list_devices() {
        // This may fail on CI without audio devices, but shouldn't panic
        let result = AudioEngine::list_devices();
        match result {
            Ok(devices) => {
                println!("Found {} devices", devices.len());
                for device in &devices {
                    println!(
                        "  - {} (in:{}, out:{})",
                        device.name, device.input_channels, device.output_channels
                    );
                }
            }
            Err(e) => {
                println!("No audio devices available: {}", e);
            }
        }
    }

    #[test]
    fn test_analysis_result_from_latency() {
        let lr = LatencyResult {
            latency_ms: 5.0,
            latency_samples: 480,
            confidence: 0.8,
            timestamp: Instant::now(),
        };

        let ar: AnalysisResult = lr.into();
        assert_eq!(ar.latency_ms, 5.0);
        assert_eq!(ar.latency_samples, 480);
        assert_eq!(ar.confidence, 0.8);
        assert!(ar.is_healthy);
    }
}
