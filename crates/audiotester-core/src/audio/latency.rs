//! Frame-based latency calculation
//!
//! Calculates audio latency using sample frame counters instead of wall-clock
//! timestamps. Both ASIO input and output callbacks share the same sample clock,
//! making frame arithmetic the authoritative timing reference.
//!
//! This eliminates the artificial delays caused by ring buffer accumulation
//! that plagued the previous timestamp-based approach.

use std::collections::VecDeque;
use std::time::Instant;

use super::burst::{BurstEvent, DetectionEvent};
use super::detector::BurstDetector;

/// Maximum number of pending bursts to track
const MAX_PENDING_BURSTS: usize = 16;

/// Maximum latency in frames before discarding a burst (500ms at 96kHz)
const MAX_LATENCY_FRAMES: u64 = 48000; // 500ms at 96kHz

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

/// Frame-based latency analyzer
///
/// Tracks burst generation events and matches them with detected bursts
/// using frame counters for sample-accurate latency calculation.
///
/// # Architecture
///
/// Both ASIO input and output callbacks share the same sample clock.
/// When output callback generates a burst at output_frame N, and
/// input callback detects it at input_frame M, the latency is:
///
/// ```text
/// latency_samples = M - N
/// latency_ms = (M - N) / sample_rate * 1000
/// ```
///
/// This is how professional tools like Ableton and RTL Utility measure latency.
///
/// # Example
/// ```
/// use audiotester_core::audio::latency::LatencyAnalyzer;
/// use audiotester_core::audio::burst::{BurstEvent, DetectionEvent};
///
/// let mut analyzer = LatencyAnalyzer::new(96000);
///
/// // Register a burst at output frame 1000
/// let event = BurstEvent { start_frame: 1000 };
/// analyzer.register_burst(event);
///
/// // Burst detected at input frame 1192 (2ms latency at 96kHz)
/// let detection = DetectionEvent { input_frame: 1192 };
/// if let Some(result) = analyzer.match_detection(&detection) {
///     assert!((result.latency_ms - 2.0).abs() < 0.1);
/// }
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
    /// * `event` - Burst event with output frame counter
    pub fn register_burst(&mut self, event: BurstEvent) {
        // Limit queue size (oldest bursts are discarded)
        while self.pending_bursts.len() >= MAX_PENDING_BURSTS {
            self.pending_bursts.pop_front();
        }

        let frame = event.start_frame;
        self.pending_bursts.push_back(event);
        tracing::trace!(
            frame = frame,
            pending = self.pending_bursts.len(),
            "burst_registered"
        );
    }

    /// Match a detection event with a pending burst using frame arithmetic
    ///
    /// # Arguments
    /// * `detection` - Detection event with input frame counter
    ///
    /// # Returns
    /// Latency result if a matching burst was found
    pub fn match_detection(&mut self, detection: &DetectionEvent) -> Option<LatencyResult> {
        // Find the NEWEST burst within latency window. Using newest-first
        // naturally handles signal recovery: after a period of no detections,
        // stale bursts have large frame diffs and are skipped, while the
        // most recent burst matches with the correct latency.
        let max_latency_frames = MAX_LATENCY_FRAMES;

        let mut matched_index = None;

        for (i, burst) in self.pending_bursts.iter().enumerate().rev() {
            if detection.input_frame >= burst.start_frame {
                let diff = detection.input_frame - burst.start_frame;
                if diff < max_latency_frames {
                    matched_index = Some(i);
                    break; // Match with newest valid burst
                }
            }
        }

        if let Some(i) = matched_index {
            let burst = self.pending_bursts.remove(i).unwrap();
            // Discard all older bursts (they're stale)
            let drain_count = i.min(self.pending_bursts.len());
            self.pending_bursts.drain(..drain_count);
            let result = self.calculate_latency_from_frames(&burst, detection);
            tracing::debug!(
                detection_frame = detection.input_frame,
                burst_frame = burst.start_frame,
                frame_diff = detection.input_frame - burst.start_frame,
                latency_ms = %format!("{:.6}", result.latency_ms),
                pending_after = self.pending_bursts.len(),
                measurement = self.measurement_count,
                "latency_matched"
            );
            self.last_result = Some(result.clone());
            return Some(result);
        }

        // Clean up stale bursts (too old to match)
        self.pending_bursts.retain(|b| {
            detection.input_frame.saturating_sub(b.start_frame) < max_latency_frames * 2
        });

        tracing::trace!(
            detection_frame = detection.input_frame,
            pending_count = self.pending_bursts.len(),
            "no_burst_match"
        );

        None
    }

    /// Calculate latency from frame counters
    ///
    /// This is the core of the frame-based approach:
    /// latency = (input_frame - output_frame) / sample_rate
    fn calculate_latency_from_frames(
        &mut self,
        burst_event: &BurstEvent,
        detection: &DetectionEvent,
    ) -> LatencyResult {
        // Simple frame arithmetic - no timestamps needed!
        let frame_diff = detection
            .input_frame
            .saturating_sub(burst_event.start_frame);

        let latency_samples = frame_diff as usize;
        let latency_ms = (frame_diff as f64 / self.sample_rate as f64) * 1000.0;

        // Update running average
        if self.measurement_count == 0 {
            self.latency_average = latency_ms;
        } else {
            self.latency_average =
                self.latency_average * (1.0 - self.average_alpha) + latency_ms * self.average_alpha;
        }
        self.measurement_count += 1;

        // Confidence based on stability
        let stability_confidence = if self.measurement_count > 5 {
            // Reduce confidence if current measurement differs significantly from average
            let deviation = (latency_ms - self.latency_average).abs();
            let relative_deviation = deviation / self.latency_average.max(1.0);
            (1.0 - relative_deviation.min(1.0) as f32).max(0.0)
        } else {
            0.5 // Lower confidence during warmup
        };

        // Frame-based measurements are inherently accurate
        // Only reduce confidence for instability
        let confidence = (0.8 + stability_confidence * 0.2).min(1.0);

        LatencyResult {
            latency_ms,
            latency_samples,
            confidence,
            timestamp: Instant::now(),
        }
    }

    /// Analyze input samples for burst detection (legacy interface)
    ///
    /// This method is kept for backward compatibility but should be avoided.
    /// Use `match_detection()` with frame-based DetectionEvent instead.
    ///
    /// # Arguments
    /// * `samples` - Buffer of input samples (channel 0 - burst signal)
    /// * `_callback_time` - Ignored (frame-based timing is used instead)
    ///
    /// # Returns
    /// Latency result if a burst was detected and matched
    #[deprecated(
        since = "0.2.0",
        note = "Use match_detection() with frame-based DetectionEvent instead"
    )]
    pub fn analyze(&mut self, samples: &[f32], _callback_time: Instant) -> Option<LatencyResult> {
        // Detect bursts in input
        let detections = self.detector.process_buffer(samples);

        if detections.is_empty() || self.pending_bursts.is_empty() {
            return None;
        }

        // For legacy compatibility, we match with oldest pending burst
        // This is NOT accurate for real-time measurement - use match_detection() instead
        if let Some(burst_event) = self.pending_bursts.pop_front() {
            // Estimate frame from detection index (imprecise without real frame counter)
            let detection = DetectionEvent {
                input_frame: burst_event.start_frame + detections[0].onset_index as u64,
            };
            let result = self.calculate_latency_from_frames(&burst_event, &detection);
            self.last_result = Some(result.clone());
            return Some(result);
        }

        None
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

    /// Clear pending (unmatched) bursts.
    ///
    /// Called when signal is lost to discard stale burst events that
    /// accumulated during the outage. Without this, recovery would match
    /// the first detection against a stale burst, producing wrong latency.
    pub fn clear_pending(&mut self) {
        self.pending_bursts.clear();
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

    #[test]
    fn test_analyzer_creation() {
        let analyzer = LatencyAnalyzer::new(96000);
        assert_eq!(analyzer.sample_rate(), 96000);
        assert_eq!(analyzer.pending_burst_count(), 0);
    }

    #[test]
    fn test_register_burst() {
        let mut analyzer = LatencyAnalyzer::new(96000);

        let event = BurstEvent { start_frame: 0 };
        analyzer.register_burst(event);

        assert_eq!(analyzer.pending_burst_count(), 1);
    }

    #[test]
    fn test_frame_based_latency_calculation() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Register a burst at output frame 1000
        let event = BurstEvent { start_frame: 1000 };
        analyzer.register_burst(event);

        // Burst detected at input frame 1240 (5ms latency at 48kHz)
        // 5ms * 48000 = 240 samples
        let detection = DetectionEvent { input_frame: 1240 };
        let result = analyzer.match_detection(&detection);

        assert!(result.is_some(), "Should match burst");
        let result = result.unwrap();
        assert_eq!(result.latency_samples, 240);
        assert!(
            (result.latency_ms - 5.0).abs() < 0.1,
            "Expected ~5ms, got {}ms",
            result.latency_ms
        );
    }

    #[test]
    fn test_frame_based_2ms_latency() {
        let mut analyzer = LatencyAnalyzer::new(96000);

        // Register a burst at output frame 5000
        let event = BurstEvent { start_frame: 5000 };
        analyzer.register_burst(event);

        // Burst detected at input frame 5192 (2ms latency at 96kHz)
        // 2ms * 96000 = 192 samples
        let detection = DetectionEvent { input_frame: 5192 };
        let result = analyzer.match_detection(&detection);

        assert!(result.is_some(), "Should match burst");
        let result = result.unwrap();
        assert_eq!(result.latency_samples, 192);
        assert!(
            (result.latency_ms - 2.0).abs() < 0.1,
            "Expected ~2ms, got {}ms",
            result.latency_ms
        );
    }

    #[test]
    fn test_no_pending_no_match() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Don't register any bursts
        let detection = DetectionEvent { input_frame: 1000 };
        let result = analyzer.match_detection(&detection);

        assert!(result.is_none(), "Should not match without pending bursts");
    }

    #[test]
    fn test_detection_before_burst_no_match() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Register a burst at frame 2000
        let event = BurstEvent { start_frame: 2000 };
        analyzer.register_burst(event);

        // Detection at frame 1000 (before burst) - shouldn't match
        let detection = DetectionEvent { input_frame: 1000 };
        let result = analyzer.match_detection(&detection);

        assert!(result.is_none(), "Detection before burst should not match");
    }

    #[test]
    fn test_stale_burst_cleanup() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Register a burst at frame 0
        let event = BurstEvent { start_frame: 0 };
        analyzer.register_burst(event);

        // Detection at frame way beyond max latency window
        // MAX_LATENCY_FRAMES is 48000 (500ms at 96kHz)
        // At 48kHz, 500ms = 24000 samples
        let detection = DetectionEvent {
            input_frame: 100000,
        };
        let result = analyzer.match_detection(&detection);

        assert!(
            result.is_none(),
            "Should not match burst outside latency window"
        );

        // Burst should be cleaned up
        assert_eq!(
            analyzer.pending_burst_count(),
            0,
            "Stale burst should be cleaned up"
        );
    }

    #[test]
    fn test_latency_averaging() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Simulate multiple measurements
        for i in 0..10 {
            let event = BurstEvent {
                start_frame: i * 1000,
            };
            analyzer.register_burst(event);

            // Detection 240 samples later (5ms)
            let detection = DetectionEvent {
                input_frame: i * 1000 + 240,
            };
            analyzer.match_detection(&detection);
        }

        assert_eq!(
            analyzer.measurement_count(),
            10,
            "Should have 10 measurements"
        );
        assert!(
            (analyzer.average_latency_ms() - 5.0).abs() < 0.5,
            "Average should be ~5ms, got {}ms",
            analyzer.average_latency_ms()
        );
    }

    #[test]
    fn test_reset() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Add some state
        let event = BurstEvent { start_frame: 0 };
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
                start_frame: i as u64,
            };
            analyzer.register_burst(event);
        }

        // Should be capped at MAX_PENDING_BURSTS
        assert!(analyzer.pending_burst_count() <= MAX_PENDING_BURSTS);
    }

    #[test]
    fn test_high_confidence_for_stable_measurements() {
        let mut analyzer = LatencyAnalyzer::new(48000);

        // Simulate many stable measurements
        for i in 0..20 {
            let event = BurstEvent {
                start_frame: i * 1000,
            };
            analyzer.register_burst(event);

            // Consistent 5ms latency
            let detection = DetectionEvent {
                input_frame: i * 1000 + 240,
            };
            let result = analyzer.match_detection(&detection);

            if let Some(r) = result {
                // After warmup, confidence should be high
                if i > 5 {
                    assert!(
                        r.confidence > 0.8,
                        "Stable measurements should have high confidence, got {}",
                        r.confidence
                    );
                }
            }
        }
    }
}
