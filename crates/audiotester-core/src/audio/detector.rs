//! Envelope-based burst detection for latency measurement
//!
//! Detects the onset of noise bursts in received audio using an envelope
//! follower with fast attack and slow release. This enables precise
//! identification of when a burst arrives for timestamp-based latency calculation.

/// Detection result from the burst detector
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Sample index within the buffer where burst was detected
    pub onset_index: usize,
    /// Current envelope level at detection
    pub envelope_level: f32,
    /// Signal-to-noise ratio estimate
    pub snr_estimate: f32,
}

/// Envelope-based burst detector
///
/// Uses an envelope follower with fast attack and slow release to detect
/// rising edges of noise bursts. The detector maintains state to track
/// the noise floor and detect when signal energy rises above a threshold.
///
/// # Example
/// ```
/// use audiotester_core::audio::detector::BurstDetector;
///
/// let mut detector = BurstDetector::new(96000);
///
/// // Process silence
/// for i in 0..100 {
///     assert!(detector.process(0.0, i).is_none());
/// }
///
/// // Process burst (should detect onset)
/// let result = detector.process(0.5, 100);
/// assert!(result.is_some());
/// ```
#[derive(Debug)]
pub struct BurstDetector {
    /// Sample rate in Hz
    sample_rate: u32,
    /// Current envelope level
    envelope: f32,
    /// Estimated noise floor
    noise_floor: f32,
    /// Threshold ratio above noise floor for detection
    threshold_ratio: f32,
    /// Whether we're currently in detected state (burst active)
    detected: bool,
    /// Attack coefficient (fast rise)
    attack_coeff: f32,
    /// Release coefficient (slow fall)
    release_coeff: f32,
    /// Noise floor adaptation coefficient
    noise_adapt_coeff: f32,
    /// Minimum samples between detections (debounce)
    min_gap_samples: usize,
    /// Samples since last detection
    samples_since_detection: usize,
    /// Peak envelope during current burst
    peak_envelope: f32,
}

impl BurstDetector {
    /// Create a new burst detector
    ///
    /// # Arguments
    /// * `sample_rate` - Sample rate in Hz
    ///
    /// # Example
    /// ```
    /// use audiotester_core::audio::detector::BurstDetector;
    ///
    /// let detector = BurstDetector::new(96000);
    /// ```
    pub fn new(sample_rate: u32) -> Self {
        // Time constants tuned for 10ms bursts every 100ms
        // Attack: ~0.5ms for quick onset detection
        // Release: ~10ms for smooth envelope during burst
        let attack_time_ms = 0.5;
        let release_time_ms = 10.0;
        let noise_adapt_time_ms = 100.0;

        let attack_coeff = Self::time_to_coeff(attack_time_ms, sample_rate);
        let release_coeff = Self::time_to_coeff(release_time_ms, sample_rate);
        let noise_adapt_coeff = Self::time_to_coeff(noise_adapt_time_ms, sample_rate);

        // Minimum gap between detections (80ms - safely within 100ms cycle)
        let min_gap_samples = (sample_rate as f64 * 0.08) as usize;

        Self {
            sample_rate,
            envelope: 0.0,
            noise_floor: 0.001,    // Initial small value to avoid division by zero
            threshold_ratio: 10.0, // Burst must be 10x above noise floor
            detected: false,
            attack_coeff,
            release_coeff,
            noise_adapt_coeff,
            min_gap_samples,
            samples_since_detection: min_gap_samples, // Allow immediate first detection
            peak_envelope: 0.0,
        }
    }

    /// Convert time constant to exponential coefficient
    fn time_to_coeff(time_ms: f32, sample_rate: u32) -> f32 {
        let samples = time_ms * sample_rate as f32 / 1000.0;
        (-1.0 / samples).exp()
    }

    /// Process a single sample
    ///
    /// Returns detection result if a burst onset was detected at this sample.
    ///
    /// # Arguments
    /// * `sample` - Audio sample value
    /// * `index` - Sample index within buffer (for detection result)
    ///
    /// # Returns
    /// `Some(DetectionResult)` if burst onset detected, `None` otherwise
    pub fn process(&mut self, sample: f32, index: usize) -> Option<DetectionResult> {
        let abs = sample.abs();
        self.samples_since_detection += 1;

        // Envelope follower with fast attack, slow release
        if abs > self.envelope {
            // Fast attack
            self.envelope = self.envelope * self.attack_coeff + abs * (1.0 - self.attack_coeff);
        } else {
            // Slow release
            self.envelope = self.envelope * self.release_coeff + abs * (1.0 - self.release_coeff);
        }

        // Track peak during burst
        if self.detected {
            self.peak_envelope = self.peak_envelope.max(self.envelope);
        }

        // Detection threshold
        let threshold = self.noise_floor.max(0.001) * self.threshold_ratio;

        // Rising edge detection with debounce
        if !self.detected
            && self.envelope > threshold
            && self.samples_since_detection >= self.min_gap_samples
        {
            self.detected = true;
            self.samples_since_detection = 0;
            self.peak_envelope = self.envelope;

            let snr_estimate = if self.noise_floor > 0.0001 {
                (self.envelope / self.noise_floor).log10() * 20.0
            } else {
                60.0 // Very clean signal
            };

            return Some(DetectionResult {
                onset_index: index,
                envelope_level: self.envelope,
                snr_estimate,
            });
        }

        // Falling edge - return to non-detected state and update noise floor
        let release_threshold = threshold * 0.5; // Hysteresis
        if self.detected && self.envelope < release_threshold {
            self.detected = false;
            // Update noise floor during silence (slow adaptation)
            self.noise_floor =
                self.noise_floor * self.noise_adapt_coeff + abs * (1.0 - self.noise_adapt_coeff);
        }

        // Always slowly adapt noise floor during non-burst periods
        if !self.detected {
            self.noise_floor =
                self.noise_floor * self.noise_adapt_coeff + abs * (1.0 - self.noise_adapt_coeff);
        }

        None
    }

    /// Process a buffer of samples
    ///
    /// Returns detection results for all burst onsets found in the buffer.
    ///
    /// # Arguments
    /// * `samples` - Buffer of audio samples
    ///
    /// # Returns
    /// Vector of detection results
    pub fn process_buffer(&mut self, samples: &[f32]) -> Vec<DetectionResult> {
        let mut results = Vec::new();
        for (i, &sample) in samples.iter().enumerate() {
            if let Some(result) = self.process(sample, i) {
                results.push(result);
            }
        }
        results
    }

    /// Get SNR confidence (0.0 to 1.0)
    ///
    /// Higher values indicate cleaner signal detection.
    pub fn snr_confidence(&self) -> f32 {
        if self.noise_floor < 0.0001 {
            return 1.0;
        }
        let snr_db = (self.peak_envelope / self.noise_floor).log10() * 20.0;
        // Map 20-60 dB SNR to 0.0-1.0 confidence
        ((snr_db - 20.0) / 40.0).clamp(0.0, 1.0)
    }

    /// Check if currently in detected (burst active) state
    pub fn is_detected(&self) -> bool {
        self.detected
    }

    /// Get current envelope level
    pub fn envelope(&self) -> f32 {
        self.envelope
    }

    /// Get current noise floor estimate
    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }

    /// Get detection threshold
    pub fn threshold(&self) -> f32 {
        self.noise_floor.max(0.001) * self.threshold_ratio
    }

    /// Set threshold ratio
    ///
    /// Higher values require stronger bursts for detection.
    /// Default is 10.0 (burst must be 10x above noise floor).
    pub fn set_threshold_ratio(&mut self, ratio: f32) {
        self.threshold_ratio = ratio.max(2.0);
    }

    /// Reset detector state
    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.noise_floor = 0.001;
        self.detected = false;
        self.samples_since_detection = self.min_gap_samples;
        self.peak_envelope = 0.0;
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_creation() {
        let detector = BurstDetector::new(96000);
        assert_eq!(detector.sample_rate(), 96000);
        assert!(!detector.is_detected());
    }

    #[test]
    fn test_silence_no_detection() {
        let mut detector = BurstDetector::new(48000);

        // Process silence
        for i in 0..1000 {
            let result = detector.process(0.0, i);
            assert!(result.is_none(), "Silence should not trigger detection");
        }

        assert!(!detector.is_detected());
    }

    #[test]
    fn test_burst_detection() {
        let mut detector = BurstDetector::new(48000);

        // Process silence to establish noise floor
        for i in 0..1000 {
            detector.process(0.0, i);
        }

        // Process burst onset
        let mut detection_idx = None;
        for i in 0..100 {
            // Sudden jump to 0.5 amplitude
            if let Some(result) = detector.process(0.5, 1000 + i) {
                detection_idx = Some(result.onset_index);
                break;
            }
        }

        assert!(
            detection_idx.is_some(),
            "Burst should be detected within 100 samples"
        );
        assert!(detector.is_detected());
    }

    #[test]
    fn test_burst_release() {
        let mut detector = BurstDetector::new(48000);

        // Establish noise floor
        for i in 0..1000 {
            detector.process(0.0, i);
        }

        // Start burst
        for i in 0..100 {
            detector.process(0.5, 1000 + i);
        }
        assert!(detector.is_detected());

        // End burst - process extended silence (10000 samples = ~200ms at 48kHz)
        // This ensures the envelope has fully decayed below the release threshold
        for i in 0..10000 {
            detector.process(0.0, 1100 + i);
        }

        // Should have released
        assert!(
            !detector.is_detected(),
            "Detector should release after extended silence, envelope: {}, threshold: {}",
            detector.envelope(),
            detector.threshold()
        );
    }

    #[test]
    fn test_debounce() {
        let mut detector = BurstDetector::new(48000);
        let min_gap = detector.min_gap_samples;

        // First burst
        for i in 0..100 {
            detector.process(0.5, i);
        }

        // Quick second burst (should be debounced)
        for i in 0..100 {
            detector.process(0.0, 100 + i); // Brief silence
        }

        let mut detected_in_debounce = false;
        for i in 0..100 {
            if detector.process(0.5, 200 + i).is_some() && i + 200 < min_gap {
                detected_in_debounce = true;
            }
        }

        assert!(
            !detected_in_debounce,
            "Should not detect within debounce period"
        );
    }

    #[test]
    fn test_snr_confidence() {
        let mut detector = BurstDetector::new(48000);

        // Process silence
        for i in 0..1000 {
            detector.process(0.0, i);
        }

        // Process strong burst
        for i in 0..100 {
            detector.process(0.8, 1000 + i);
        }

        let confidence = detector.snr_confidence();
        assert!(
            confidence > 0.5,
            "Strong burst should have high confidence, got {}",
            confidence
        );
    }

    #[test]
    fn test_process_buffer() {
        let mut detector = BurstDetector::new(48000);

        // Create buffer with silence then burst
        let mut buffer = vec![0.0f32; 2000];
        for sample in buffer.iter_mut().skip(1000) {
            *sample = 0.5;
        }

        let results = detector.process_buffer(&buffer);

        assert_eq!(results.len(), 1, "Should detect exactly one burst");
        assert!(
            results[0].onset_index >= 1000,
            "Detection should be at or after burst start"
        );
    }

    #[test]
    fn test_reset() {
        let mut detector = BurstDetector::new(48000);

        // Process some samples
        for i in 0..100 {
            detector.process(0.5, i);
        }

        detector.reset();

        assert!(!detector.is_detected());
        assert!((detector.envelope() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_threshold_adjustment() {
        let mut detector = BurstDetector::new(48000);

        // Lower threshold should detect smaller signals
        detector.set_threshold_ratio(5.0);
        let low_threshold = detector.threshold();

        detector.set_threshold_ratio(20.0);
        let high_threshold = detector.threshold();

        assert!(
            high_threshold > low_threshold,
            "Higher ratio should give higher threshold"
        );
    }

    #[test]
    fn test_noise_adaptation() {
        let mut detector = BurstDetector::new(48000);
        let initial_floor = detector.noise_floor();

        // Process some low-level noise
        for i in 0..10000 {
            let noise = ((i as f32 * 0.1).sin()) * 0.01; // Small sine "noise"
            detector.process(noise, i);
        }

        // Noise floor should have adapted
        let adapted_floor = detector.noise_floor();
        assert!(
            adapted_floor > initial_floor,
            "Noise floor should adapt upward with noise present"
        );
    }
}
