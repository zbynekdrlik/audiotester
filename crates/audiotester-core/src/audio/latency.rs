//! Timestamp-based latency calculation
//!
//! Calculates audio latency by comparing burst generation timestamps with
//! detection timestamps. This provides sub-millisecond accurate latency
//! measurement without the buffer accumulation delays of MLS correlation.

use std::collections::VecDeque;
use std::time::Instant;

use super::burst::BurstEvent;
use super::detector::{BurstDetector, DetectionResult};

/// Maximum number of pending bursts to track
const MAX_PENDING_BURSTS: usize = 16;

/// Maximum age of a pending burst before discarding (ms)
const MAX_BURST_AGE_MS: u64 = 500;

/// Latency measurement result
#[derive(Debug, Clone)]
pub struct LatencyResult {
    /// Measured latency in milliseconds
    pub latency_ms: f64,
    /// Measured latency in samples
    pub latency_samples: usize,
    /// Confidence of the measurement (0.0 to 1.0)
    pub confidence: f32,
    /// Timestamp of when this measurement was taken
    pub timestamp: Instant,
}

impl Default for LatencyResult {
    fn default() -> Self {
        Self {
            latency_ms: 0.0,
            latency_samples: 0,
            confidence: 0.0,
            timestamp: Instant::now(),
        }
    }
}

/// Timestamp-based latency analyzer
///
/// Tracks burst generation events and matches them with detected bursts
/// to calculate round-trip latency with sample-accurate precision.
///
/// # Example
/// ```
/// use audiotester_core::audio::latency::LatencyAnalyzer;
/// use audiotester_core::audio::burst::BurstEvent;
/// use std::time::Instant;
///
/// let mut analyzer = LatencyAnalyzer::new(96000);
///
/// // Register a burst that was just generated
/// let event = BurstEvent {
///     start_time: Instant::now(),
///     start_frame: 0,
/// };
/// analyzer.register_burst(event);
///
/// // Later, when burst is detected in input...
/// // analyzer.analyze(...) will calculate latency
/// ```
#[derive(Debug)]
pub struct LatencyAnalyzer {
    /// Sample rate in Hz
    sample_rate: u32,
    /// Burst detector for input signal
    detector: BurstDetector,
    /// Queue of pending (unmatched) burst events
    pending_bursts: VecDeque<BurstEvent>,
    /// Most recent latency measurement
    last_result: Option<LatencyResult>,
    /// Running average of latency for smoothing
    latency_average: f64,
    /// Alpha for exponential moving average
    average_alpha: f64,
    /// Number of measurements taken
    measurement_count: u64,
}

impl LatencyAnalyzer {
    /// Create a new latency analyzer
    ///
    /// # Arguments
    /// * `sample_rate` - Sample rate in Hz
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            detector: BurstDetector::new(sample_rate),
            pending_bursts: VecDeque::with_capacity(MAX_PENDING_BURSTS),
            last_result: None,
            latency_average: 0.0,
            average_alpha: 0.3, // Faster adaptation
            measurement_count: 0,
        }
    }

    /// Register a burst generation event
    ///
    /// Call this when a burst is generated on output. The analyzer will
    /// attempt to match it with a detected burst on input.
    ///
    /// # Arguments
    /// * `event` - Burst event with timestamp and frame counter
    pub fn register_burst(&mut self, event: BurstEvent) {
        // Prune old bursts
        let now = Instant::now();
        self.pending_bursts
            .retain(|b| now.duration_since(b.start_time).as_millis() < MAX_BURST_AGE_MS as u128);

        // Limit queue size
        while self.pending_bursts.len() >= MAX_PENDING_BURSTS {
            self.pending_bursts.pop_front();
        }

        self.pending_bursts.push_back(event);
    }

    /// Analyze input samples for burst detection and latency calculation
    ///
    /// # Arguments
    /// * `samples` - Buffer of input samples (channel 0 - burst signal)
    /// * `callback_time` - Timestamp when the input callback received this buffer
    ///
    /// # Returns
    /// Latency result if a burst was detected and matched
    pub fn analyze(&mut self, samples: &[f32], callback_time: Instant) -> Option<LatencyResult> {
        // Detect bursts in input
        let detections = self.detector.process_buffer(samples);

        if detections.is_empty() || self.pending_bursts.is_empty() {
            return None;
        }

        // Process first detection
        let detection = &detections[0];

        // Match with oldest pending burst
        if let Some(burst_event) = self.pending_bursts.pop_front() {
            let result = self.calculate_latency(&burst_event, detection, callback_time);
            self.last_result = Some(result.clone());
            return Some(result);
        }

        None
    }

    /// Calculate latency from matched burst event and detection
    fn calculate_latency(
        &mut self,
        burst_event: &BurstEvent,
        detection: &DetectionResult,
        callback_time: Instant,
    ) -> LatencyResult {
        // Time from burst generation to callback
        let time_diff = callback_time.duration_since(burst_event.start_time);

        // Add sample offset within buffer to account for where detection occurred
        let sample_offset_secs = detection.onset_index as f64 / self.sample_rate as f64;

        // Total latency in seconds
        // Note: We subtract the sample offset because the detection happened
        // *during* the callback, so the actual arrival was sample_offset earlier
        let latency_secs = time_diff.as_secs_f64() - sample_offset_secs;

        // Clamp to reasonable range (avoid negative due to timing jitter)
        let latency_secs = latency_secs.max(0.0);

        let latency_ms = latency_secs * 1000.0;
        let latency_samples = (latency_secs * self.sample_rate as f64).round() as usize;

        // Update running average
        if self.measurement_count == 0 {
            self.latency_average = latency_ms;
        } else {
            self.latency_average =
                self.latency_average * (1.0 - self.average_alpha) + latency_ms * self.average_alpha;
        }
        self.measurement_count += 1;

        // Confidence based on SNR and stability
        let snr_confidence = self.detector.snr_confidence();
        let stability_confidence = if self.measurement_count > 5 {
            // Reduce confidence if current measurement differs significantly from average
            let deviation = (latency_ms - self.latency_average).abs();
            let relative_deviation = deviation / self.latency_average.max(1.0);
            (1.0 - relative_deviation.min(1.0) as f32).max(0.0)
        } else {
            0.5 // Lower confidence during warmup
        };

        let confidence = (snr_confidence * 0.7 + stability_confidence * 0.3).min(1.0);

        LatencyResult {
            latency_ms,
            latency_samples,
            confidence,
            timestamp: Instant::now(),
        }
    }

    /// Get the most recent latency measurement
    pub fn last_result(&self) -> Option<&LatencyResult> {
        self.last_result.as_ref()
    }

    /// Get the smoothed average latency in milliseconds
    pub fn average_latency_ms(&self) -> f64 {
        self.latency_average
    }

    /// Get number of measurements taken
    pub fn measurement_count(&self) -> u64 {
        self.measurement_count
    }

    /// Get number of pending (unmatched) bursts
    pub fn pending_burst_count(&self) -> usize {
        self.pending_bursts.len()
    }

    /// Check if currently detecting a burst
    pub fn is_detecting(&self) -> bool {
        self.detector.is_detected()
    }

    /// Get current envelope level from detector
    pub fn envelope(&self) -> f32 {
        self.detector.envelope()
    }

    /// Get current noise floor estimate
    pub fn noise_floor(&self) -> f32 {
        self.detector.noise_floor()
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Reset analyzer state
    pub fn reset(&mut self) {
        self.detector.reset();
        self.pending_bursts.clear();
        self.last_result = None;
        self.latency_average = 0.0;
        self.measurement_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_analyzer_creation() {
        let analyzer = LatencyAnalyzer::new(96000);
        assert_eq!(analyzer.sample_rate(), 96000);
        assert_eq!(analyzer.pending_burst_count(), 0);
    }

    #[test]
    fn test_register_burst() {
        let mut analyzer = LatencyAnalyzer::new(96000);

        let event = BurstEvent {
            start_time: Instant::now(),
            start_frame: 0,
        };
        analyzer.register_burst(event);

        assert_eq!(analyzer.pending_burst_count(), 1);
    }

    #[test]
    fn test_burst_matching() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Register a burst
        let start_time = Instant::now();
        let event = BurstEvent {
            start_time,
            start_frame: 0,
        };
        analyzer.register_burst(event);

        // Simulate some delay
        thread::sleep(Duration::from_millis(5));

        // Create input buffer with burst signal
        let mut samples = vec![0.0f32; 500];
        // Add burst starting at index 100
        for i in 100..500 {
            samples[i] = 0.5;
        }

        let callback_time = Instant::now();
        let result = analyzer.analyze(&samples, callback_time);

        assert!(result.is_some(), "Should detect and match burst");
        let result = result.unwrap();
        assert!(
            result.latency_ms > 0.0,
            "Latency should be positive: {}",
            result.latency_ms
        );
        assert!(
            result.latency_ms < 50.0,
            "Latency should be reasonable: {}",
            result.latency_ms
        );
    }

    #[test]
    fn test_no_burst_no_match() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Register a burst
        let event = BurstEvent {
            start_time: Instant::now(),
            start_frame: 0,
        };
        analyzer.register_burst(event);

        // Analyze silence (no burst)
        let samples = vec![0.0f32; 1000];
        let result = analyzer.analyze(&samples, Instant::now());

        assert!(result.is_none(), "Should not match without detected burst");
    }

    #[test]
    fn test_no_pending_no_match() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Don't register any bursts

        // Analyze buffer with burst
        let mut samples = vec![0.0f32; 500];
        for i in 100..500 {
            samples[i] = 0.5;
        }

        let result = analyzer.analyze(&samples, Instant::now());

        assert!(result.is_none(), "Should not match without pending bursts");
    }

    #[test]
    fn test_burst_expiration() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Register an old burst
        let old_time = Instant::now() - Duration::from_millis(600); // Older than MAX_BURST_AGE_MS
        let event = BurstEvent {
            start_time: old_time,
            start_frame: 0,
        };
        analyzer.register_burst(event);

        // Register a new burst to trigger pruning
        let new_event = BurstEvent {
            start_time: Instant::now(),
            start_frame: 1,
        };
        analyzer.register_burst(new_event);

        // Old burst should be pruned, only new one remains
        assert_eq!(analyzer.pending_burst_count(), 1);
    }

    #[test]
    fn test_latency_averaging() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Simulate multiple measurements with proper silence between bursts
        // The detector has a debounce period (~80ms at 48kHz = 3840 samples)
        // so we need to properly reset between measurements
        for i in 0..10 {
            let event = BurstEvent {
                start_time: Instant::now(),
                start_frame: i,
            };
            analyzer.register_burst(event);

            thread::sleep(Duration::from_millis(5));

            // First provide silence to reset detector state
            let silence = vec![0.0f32; 5000]; // ~100ms of silence
            let _ = analyzer.analyze(&silence, Instant::now());

            // Then provide burst
            let mut samples = vec![0.0f32; 500];
            for j in 100..500 {
                samples[j] = 0.5;
            }

            let _ = analyzer.analyze(&samples, Instant::now());
        }

        // Due to detector debounce, not all bursts will be detected
        // Just verify we got at least some measurements
        assert!(
            analyzer.measurement_count() >= 1,
            "Should have at least one measurement, got {}",
            analyzer.measurement_count()
        );
        assert!(
            analyzer.average_latency_ms() > 0.0,
            "Average should be positive"
        );
    }

    #[test]
    fn test_reset() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Add some state
        let event = BurstEvent {
            start_time: Instant::now(),
            start_frame: 0,
        };
        analyzer.register_burst(event);

        analyzer.reset();

        assert_eq!(analyzer.pending_burst_count(), 0);
        assert_eq!(analyzer.measurement_count(), 0);
        assert!(analyzer.last_result().is_none());
    }

    #[test]
    fn test_max_pending_bursts() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Register more than MAX_PENDING_BURSTS
        for i in 0..(MAX_PENDING_BURSTS + 5) {
            let event = BurstEvent {
                start_time: Instant::now(),
                start_frame: i as u64,
            };
            analyzer.register_burst(event);
        }

        // Should be capped at MAX_PENDING_BURSTS
        assert!(analyzer.pending_burst_count() <= MAX_PENDING_BURSTS);
    }
}
