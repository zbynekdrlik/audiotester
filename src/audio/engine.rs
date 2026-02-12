//! ASIO audio engine for device management and stream handling
//!
//! Provides high-level interface for:
//! - Enumerating ASIO devices
//! - Opening input/output streams
//! - Managing audio callbacks

use crate::audio::analyzer::{AnalysisResult, Analyzer};
use crate::audio::signal::MlsGenerator;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleRate, Stream, StreamConfig};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Ring buffer size in samples (enough for ~1 second at 48kHz)
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

/// Shared state between audio callbacks and main thread
struct SharedState {
    /// MLS generator for output
    generator: Mutex<MlsGenerator>,
    /// Signal analyzer
    analyzer: Mutex<Analyzer>,
    /// Latest analysis result
    last_result: Mutex<Option<AnalysisResult>>,
    /// Running flag
    running: AtomicBool,
    /// Counter for output samples (debug)
    output_samples: std::sync::atomic::AtomicUsize,
    /// Counter for input samples (debug)
    input_samples: std::sync::atomic::AtomicUsize,
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
    /// Consumer for input samples (analysis reads)
    input_consumer: Option<ringbuf::HeapCons<f32>>,
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
            input_consumer: None,
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
        self.sample_rate = rate;
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
    /// generating test signals and analyzing received audio.
    pub fn start(&mut self) -> Result<()> {
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| anyhow!("No device selected"))?;

        // Get device's default output config - this tells us what sample rate the device supports
        let default_output = device.default_output_config();
        let default_input = device.default_input_config();

        // Log what the device reports
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

        // Always use configured sample rate (default 96kHz)
        // ASIO devices like VASIO-8 may report incorrect default rates (44100Hz)
        // but actually run at the rate configured in their control panel (96kHz)
        let actual_sample_rate = self.sample_rate;
        tracing::info!("Using configured sample rate: {} Hz", actual_sample_rate);

        // Log warning if device reports a different rate
        if let Ok(ref cfg) = default_output {
            let device_rate = cfg.sample_rate().0;
            if device_rate > 0 && device_rate != actual_sample_rate {
                tracing::warn!(
                    "Device reports default {} Hz, but using configured {} Hz. \
                     Ensure VBMatrix/ASIO control panel is set to {} Hz.",
                    device_rate,
                    actual_sample_rate,
                    actual_sample_rate
                );
            }
        }

        // Get device channel counts
        let output_channels = default_output.as_ref().map(|c| c.channels()).unwrap_or(2);
        let input_channels = default_input.as_ref().map(|c| c.channels()).unwrap_or(2);

        tracing::info!(
            "Device channel count: {} output, {} input",
            output_channels,
            input_channels
        );

        // Create output stream config with all device channels
        // Signal will be sent on channel 0 (user's "channel 1"), others get silence
        let output_config = StreamConfig {
            channels: output_channels,
            sample_rate: SampleRate(actual_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Create input stream config (mono is fine for receiving loopback)
        let input_config = StreamConfig {
            channels: 1,
            sample_rate: SampleRate(actual_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Create ring buffer for input samples
        let ring = HeapRb::<f32>::new(RING_BUFFER_SIZE);
        let (producer, consumer) = ring.split();

        // Create shared state
        let generator = MlsGenerator::new(crate::MLS_ORDER);
        let reference = generator.sequence().to_vec();
        let analyzer = Analyzer::new(&reference, self.sample_rate);

        let shared_state = Arc::new(SharedState {
            generator: Mutex::new(generator),
            analyzer: Mutex::new(analyzer),
            last_result: Mutex::new(None),
            running: AtomicBool::new(true),
            output_samples: std::sync::atomic::AtomicUsize::new(0),
            input_samples: std::sync::atomic::AtomicUsize::new(0),
        });

        // Create output stream with multi-channel support
        let output_state = Arc::clone(&shared_state);
        let num_output_channels = output_channels as usize;
        let output_stream = device.build_output_stream(
            &output_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if output_state.running.load(Ordering::Relaxed) {
                    if let Ok(mut gen) = output_state.generator.lock() {
                        // Data is interleaved: [ch0, ch1, ch2, ..., ch(n-1), ch0, ch1, ...]
                        // We put MLS signal on channel 0 (user's "channel 1"), silence on others
                        let mut frame_count = 0usize;
                        for frame in data.chunks_mut(num_output_channels) {
                            let sample = gen.next_sample();
                            if !frame.is_empty() {
                                frame[0] = sample; // Channel 1 (index 0) gets signal
                            }
                            // Fill remaining channels with silence
                            for ch in frame.iter_mut().skip(1) {
                                *ch = 0.0;
                            }
                            frame_count += 1;
                        }
                        // Track output samples (count frames, not total samples)
                        let prev = output_state
                            .output_samples
                            .fetch_add(frame_count, Ordering::Relaxed);
                        // Log first callback to confirm output is working
                        if prev == 0 {
                            tracing::info!(
                                "Output callback started: {} frames ({} channels), first value: {:.4}",
                                frame_count,
                                num_output_channels,
                                data.first().copied().unwrap_or(0.0)
                            );
                        }
                    }
                } else {
                    // Fill with silence when stopped
                    data.fill(0.0);
                }
            },
            move |err| {
                tracing::error!("Output stream error: {}", err);
            },
            None,
        )?;

        // Create input stream
        let input_producer = Arc::new(Mutex::new(producer));
        let input_state = Arc::clone(&shared_state);

        let input_stream = device.build_input_stream(
            &input_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if input_state.running.load(Ordering::Relaxed) {
                    if let Ok(mut prod) = input_producer.lock() {
                        // Push samples to ring buffer (drop oldest if full)
                        for &sample in data {
                            let _ = prod.try_push(sample);
                        }
                        // Track input samples for debugging
                        let prev = input_state
                            .input_samples
                            .fetch_add(data.len(), Ordering::Relaxed);
                        // Log first callback to confirm input is working
                        if prev == 0 {
                            let max_level = data.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
                            tracing::info!(
                                "Input callback started: {} samples, max level: {:.4}",
                                data.len(),
                                max_level
                            );
                        }
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
        self.input_consumer = Some(consumer);
        self.state = EngineState::Running;

        tracing::info!(
            "Audio engine started: {} @ {}Hz",
            self.device_name.as_deref().unwrap_or("unknown"),
            self.sample_rate
        );

        Ok(())
    }

    /// Stop audio processing
    pub fn stop(&mut self) -> Result<()> {
        // Signal streams to stop
        if let Some(ref state) = self.shared_state {
            state.running.store(false, Ordering::Relaxed);
        }

        // Drop streams (stops them)
        self.input_stream = None;
        self.output_stream = None;
        self.shared_state = None;
        self.input_consumer = None;

        self.state = EngineState::Stopped;

        tracing::info!("Audio engine stopped");

        Ok(())
    }

    /// Analyze buffered input samples
    ///
    /// Call this periodically from the main thread to process received audio.
    ///
    /// # Returns
    /// Analysis result if enough samples are available
    pub fn analyze(&mut self) -> Option<AnalysisResult> {
        let consumer = self.input_consumer.as_mut()?;
        let shared_state = self.shared_state.as_ref()?;

        // Need at least one MLS period for analysis
        let required_samples = crate::MLS_LENGTH + 1000; // Extra for latency

        if consumer.occupied_len() < required_samples {
            return None;
        }

        // Read samples from ring buffer
        let mut samples = vec![0.0f32; required_samples];
        let read = consumer.pop_slice(&mut samples);
        samples.truncate(read);

        // Run analysis
        if let Ok(mut analyzer) = shared_state.analyzer.lock() {
            let result = analyzer.analyze(&samples);

            // Store result
            if let Ok(mut last) = shared_state.last_result.lock() {
                *last = Some(result.clone());
            }

            return Some(result);
        }

        None
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
        self.shared_state
            .as_ref()
            .map(|s| {
                (
                    s.output_samples.load(Ordering::Relaxed),
                    s.input_samples.load(Ordering::Relaxed),
                )
            })
            .unwrap_or((0, 0))
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
    fn test_list_devices() {
        // This may fail on CI without audio devices, but shouldn't panic
        let result = AudioEngine::list_devices();
        // Just check it doesn't panic - may return empty list or error
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
}
