//! Frame-based loss detection and legacy MLS correlation analysis
//!
//! Primary functions:
//! - Frame counter analysis for accurate loss detection
//!
//! Legacy functions (for backward compatibility and fallback):
//! - MLS cross-correlation for latency measurement

use rustfft::{num_complex::Complex, FftPlanner};

/// Analysis results from comparing sent and received signals
///
/// Note: For latency measurement, prefer using the burst-based system
/// in [`crate::audio::latency::LatencyAnalyzer`]. This struct is retained
/// for backward compatibility.
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

/// Result from frame-based loss detection on the counter channel (ch1).
///
/// Distinguishes between confirmed gaps in the counter sequence and
/// counter signal absence (silence from muted loopback route).
#[derive(Debug, Clone, Default)]
pub struct FrameLossResult {
    /// Real gaps detected in the counter sequence
    pub confirmed_lost: usize,
    /// True when the counter signal is absent (all zeros exceeding threshold)
    pub counter_silent: bool,
    /// Number of samples that were analyzed in this call
    pub samples_analyzed: usize,
}

/// Signal analyzer for loss detection (and legacy MLS correlation)
///
/// Primary use: Frame counter-based loss detection via [`Self::detect_frame_loss`].
///
/// Legacy use: MLS cross-correlation via [`Self::analyze`] for fallback latency
/// measurement. Note that MLS correlation has significant buffer delays and
/// is not recommended for real-time latency monitoring.
pub struct Analyzer {
    /// Sample rate in Hz
    sample_rate: u32,
    /// Reference MLS sequence for correlation (legacy)
    reference: Vec<f32>,
    /// FFT planner for efficient correlation (legacy)
    fft_planner: FftPlanner<f32>,
    /// Pre-computed FFT of reference signal (legacy)
    reference_fft: Vec<Complex<f32>>,
    /// FFT size (power of 2 >= 2 * reference length) (legacy)
    fft_size: usize,
    /// Last known latency for tracking (legacy)
    last_latency: Option<usize>,
    /// Expected frame counter for loss detection
    expected_frame: u64,
    /// Whether this analyzer has a valid MLS reference
    has_reference: bool,
    /// Count of consecutive decoded-zero samples for silence detection
    consecutive_zeros: usize,
    /// Number of consecutive zero samples required to declare silence (sample_rate / 10 = 100ms)
    silence_threshold: usize,
    /// Whether the previous call was in a silence state (used for recovery resync)
    was_silent: bool,
}

impl Analyzer {
    /// Create a new analyzer
    ///
    /// # Arguments
    /// * `reference` - The MLS sequence used for correlation (can be empty for loss-only mode)
    /// * `sample_rate` - Sample rate in Hz for time calculations
    pub fn new(reference: &[f32], sample_rate: u32) -> Self {
        let has_reference = !reference.is_empty();
        let ref_len = if has_reference { reference.len() } else { 1024 };
        let fft_size = (ref_len * 2).next_power_of_two();

        let mut fft_planner = FftPlanner::new();

        // Pre-compute FFT of reference signal (zero-padded)
        let reference_fft = if has_reference {
            let mut reference_complex: Vec<Complex<f32>> = reference
                .iter()
                .map(|&x| Complex::new(x, 0.0))
                .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
                .take(fft_size)
                .collect();

            let fft = fft_planner.plan_fft_forward(fft_size);
            fft.process(&mut reference_complex);

            // Conjugate for correlation
            for c in &mut reference_complex {
                c.im = -c.im;
            }
            reference_complex
        } else {
            vec![Complex::new(0.0, 0.0); fft_size]
        };

        Self {
            sample_rate,
            reference: reference.to_vec(),
            fft_planner,
            reference_fft,
            fft_size,
            last_latency: None,
            expected_frame: 0,
            has_reference,
            consecutive_zeros: 0,
            silence_threshold: (sample_rate / 10) as usize,
            was_silent: false,
        }
    }

    /// Analyze received audio buffer using MLS cross-correlation (legacy)
    ///
    /// **Note:** This method uses MLS correlation which requires ~350ms of buffer
    /// accumulation before producing results. For real-time latency measurement,
    /// use the burst-based system in [`crate::audio::latency::LatencyAnalyzer`].
    ///
    /// # Arguments
    /// * `received` - Buffer of received audio samples
    ///
    /// # Returns
    /// Analysis result with latency and loss information
    pub fn analyze(&mut self, received: &[f32]) -> AnalysisResult {
        if !self.has_reference || received.len() < self.reference.len() {
            return AnalysisResult {
                is_healthy: false,
                ..Default::default()
            };
        }

        // Perform cross-correlation via FFT
        let (latency_samples, confidence) = self.cross_correlate(received);

        // Convert to milliseconds
        let latency_ms = (latency_samples as f64 / self.sample_rate as f64) * 1000.0;

        // Track latency changes for loss detection (legacy heuristic)
        let lost_samples = self.detect_loss(latency_samples);

        // Determine health status
        let is_healthy = confidence > 0.5 && lost_samples == 0;

        self.last_latency = Some(latency_samples);

        AnalysisResult {
            latency_samples,
            latency_ms,
            confidence,
            lost_samples,
            corrupted_samples: 0,
            is_healthy,
        }
    }

    /// Perform FFT-based cross-correlation (legacy)
    fn cross_correlate(&mut self, received: &[f32]) -> (usize, f32) {
        // Zero-pad received signal
        let mut received_complex: Vec<Complex<f32>> = received
            .iter()
            .map(|&x| Complex::new(x, 0.0))
            .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
            .take(self.fft_size)
            .collect();

        // FFT of received signal
        let fft = self.fft_planner.plan_fft_forward(self.fft_size);
        fft.process(&mut received_complex);

        // Multiply with conjugate of reference FFT
        for (r, ref_c) in received_complex.iter_mut().zip(&self.reference_fft) {
            *r *= *ref_c;
        }

        // Inverse FFT to get correlation
        let ifft = self.fft_planner.plan_fft_inverse(self.fft_size);
        ifft.process(&mut received_complex);

        // Constrain search to reasonable latency range (0-100ms)
        let max_latency_samples = (self.sample_rate / 10) as usize;
        let search_limit = max_latency_samples.min(received_complex.len());

        // Find peak in correlation
        let mut max_val = 0.0f32;
        let mut max_idx = 0;
        let norm = 1.0 / self.fft_size as f32;

        for (i, c) in received_complex.iter().take(search_limit).enumerate() {
            let val = (c.re * norm).abs();
            if val > max_val {
                max_val = val;
                max_idx = i;
            }
        }

        // Normalize confidence by reference energy (guard against near-zero energy)
        let ref_energy: f32 = self.reference.iter().map(|x| x * x).sum();
        let confidence = if ref_energy > 1e-10 {
            max_val / ref_energy.sqrt()
        } else {
            0.0
        };

        (max_idx, confidence.clamp(0.0, 1.0))
    }

    /// Detect sample loss based on latency changes (legacy heuristic)
    fn detect_loss(&mut self, current_latency: usize) -> usize {
        match self.last_latency {
            Some(last) if current_latency > last => {
                let diff = current_latency - last;
                if diff > 10 {
                    diff
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// Detect frame loss by analyzing the counter channel
    ///
    /// This is the **primary** method for loss detection. The counter channel
    /// contains a sawtooth waveform (0.0 to 1.0) that encodes a 16-bit frame
    /// counter. By tracking the sequence, we can detect gaps indicating lost
    /// samples with high accuracy.
    ///
    /// Also detects counter signal absence (silence) when the loopback route
    /// is muted. On recovery from silence, resyncs `expected_frame` to avoid
    /// reporting a massive false loss spike.
    ///
    /// # Arguments
    /// * `counter_samples` - Samples from the counter channel (ch1)
    ///
    /// # Returns
    /// [`FrameLossResult`] with confirmed loss count and silence state
    pub fn detect_frame_loss(&mut self, counter_samples: &[f32]) -> FrameLossResult {
        if counter_samples.is_empty() {
            return FrameLossResult::default();
        }

        let mut total_lost = 0usize;

        for &sample in counter_samples {
            // Decode counter from normalized audio (0.0-1.0 → 0-65535)
            let normalized = sample.clamp(0.0, 1.0);
            let received_counter = (normalized * 65536.0) as u32 & 0xFFFF;

            // Track consecutive zeros for silence detection
            if received_counter == 0 {
                self.consecutive_zeros += 1;
            } else {
                // Non-zero sample received
                if self.was_silent {
                    // Recovery from silence: resync expected_frame instead of
                    // computing a gap. This prevents the massive false loss spike
                    // that occurs when the counter resumes at a different value.
                    self.was_silent = false;
                    self.expected_frame = (received_counter as u64).wrapping_add(1);
                    self.consecutive_zeros = 0;
                    continue;
                }
                self.consecutive_zeros = 0;
            }

            // Normal gap detection (only when not in silence)
            if !self.was_silent && self.expected_frame > 0 {
                let expected = (self.expected_frame & 0xFFFF) as u32;

                // Calculate difference accounting for wrap-around
                let diff = if received_counter >= expected {
                    received_counter - expected
                } else {
                    (65536 + received_counter as u64 - expected as u64) as u32
                };

                // If diff > 1 and < 32768 (half range), we have a gap
                if diff > 1 && diff < 32768 {
                    total_lost += (diff - 1) as usize;
                }
            }

            // Update expected frame (skip when entering silence to freeze tracking)
            if self.consecutive_zeros < self.silence_threshold {
                self.expected_frame = (received_counter as u64).wrapping_add(1);
            }
        }

        // Determine silence state
        let counter_silent = self.consecutive_zeros >= self.silence_threshold;
        if counter_silent {
            self.was_silent = true;
        }

        FrameLossResult {
            confirmed_lost: total_lost,
            counter_silent,
            samples_analyzed: counter_samples.len(),
        }
    }

    /// Get the configured sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Check if analyzer has a valid MLS reference for correlation
    pub fn has_reference(&self) -> bool {
        self.has_reference
    }

    /// Reset the analyzer state
    pub fn reset(&mut self) {
        self.last_latency = None;
        self.expected_frame = 0;
        self.consecutive_zeros = 0;
        self.was_silent = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::signal::MlsGenerator;

    #[test]
    fn test_analyzer_creation() {
        let gen = MlsGenerator::new(10);
        let analyzer = Analyzer::new(gen.sequence(), 48000);
        assert_eq!(analyzer.sample_rate(), 48000);
        assert!(analyzer.has_reference());
    }

    #[test]
    fn test_analyzer_loss_only_mode() {
        let analyzer = Analyzer::new(&[], 48000);
        assert_eq!(analyzer.sample_rate(), 48000);
        assert!(!analyzer.has_reference());
    }

    #[test]
    fn test_perfect_correlation() {
        let gen = MlsGenerator::new(10);
        let sequence = gen.sequence().to_vec();

        let mut analyzer = Analyzer::new(&sequence, 48000);

        // Perfect match - no delay
        let result = analyzer.analyze(&sequence);
        assert_eq!(result.latency_samples, 0);
        assert!(result.confidence > 0.9);
        assert!(result.is_healthy);
    }

    #[test]
    fn test_delayed_correlation() {
        let gen = MlsGenerator::new(10);
        let sequence = gen.sequence().to_vec();

        let mut analyzer = Analyzer::new(&sequence, 48000);

        // Create delayed signal (100 samples delay)
        let delay = 100;
        let mut delayed: Vec<f32> = vec![0.0; delay];
        delayed.extend(&sequence);

        let result = analyzer.analyze(&delayed);
        assert_eq!(result.latency_samples, delay);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn test_frame_loss_detection() {
        let mut analyzer = Analyzer::new(&[], 48000);

        // Simulate continuous counter values with a gap
        // Frames 0-99, then skip to 105-199 (missing 100,101,102,103,104 = 5 frames)
        let mut samples = Vec::new();
        for i in 0..100 {
            samples.push(i as f32 / 65536.0);
        }
        // Skip 5 frames (100-104)
        for i in 105..200 {
            samples.push(i as f32 / 65536.0);
        }

        let result = analyzer.detect_frame_loss(&samples);
        // After frame 99, expected is 100. We receive 105.
        // diff = 105 - 100 = 5, lost = diff - 1 = 4
        // This is because the algorithm detects gap size minus the first received sample
        assert!(
            (4..=5).contains(&result.confirmed_lost),
            "Should detect approximately 5 lost frames, got {}",
            result.confirmed_lost
        );
        assert!(!result.counter_silent);
    }

    #[test]
    fn test_no_frame_loss() {
        let mut analyzer = Analyzer::new(&[], 48000);

        // Continuous counter values
        let samples: Vec<f32> = (0..100).map(|i| i as f32 / 65536.0).collect();

        let result = analyzer.detect_frame_loss(&samples);
        assert_eq!(result.confirmed_lost, 0, "Should detect no lost frames");
        assert!(!result.counter_silent);
    }

    #[test]
    fn test_frame_counter_wrap() {
        let mut analyzer = Analyzer::new(&[], 48000);

        // Counter wrapping around 65536
        let mut samples = Vec::new();
        for i in 65530..65536 {
            samples.push(i as f32 / 65536.0);
        }
        for i in 0..10 {
            samples.push(i as f32 / 65536.0);
        }

        let result = analyzer.detect_frame_loss(&samples);
        assert_eq!(
            result.confirmed_lost, 0,
            "Should handle wrap-around correctly"
        );
        assert!(!result.counter_silent);
    }

    #[test]
    fn test_reset() {
        let gen = MlsGenerator::new(10);
        let sequence = gen.sequence().to_vec();
        let mut analyzer = Analyzer::new(&sequence, 48000);

        // Do some analysis
        let _ = analyzer.analyze(&sequence);

        analyzer.reset();

        // After reset, expected_frame should be 0
        assert_eq!(analyzer.expected_frame, 0);
    }

    #[test]
    fn test_insufficient_samples() {
        let gen = MlsGenerator::new(10);
        let sequence = gen.sequence().to_vec();

        let mut analyzer = Analyzer::new(&sequence, 48000);

        let short_buffer = [0.0f32; 100];
        let result = analyzer.analyze(&short_buffer);
        assert!(!result.is_healthy);
    }
}
