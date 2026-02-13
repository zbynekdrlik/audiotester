//! Signal analysis for latency measurement and sample loss detection
//!
//! Uses cross-correlation to measure latency between sent and received signals,
//! and tracks frame markers to detect sample loss.

use rustfft::{num_complex::Complex, FftPlanner};

/// Analysis results from comparing sent and received signals
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

/// Signal analyzer for latency and loss detection
pub struct Analyzer {
    /// Sample rate in Hz
    sample_rate: u32,
    /// Reference MLS sequence for correlation
    reference: Vec<f32>,
    /// FFT planner for efficient correlation
    fft_planner: FftPlanner<f32>,
    /// Pre-computed FFT of reference signal
    reference_fft: Vec<Complex<f32>>,
    /// FFT size (power of 2 >= 2 * reference length)
    fft_size: usize,
    /// Last known latency for tracking
    last_latency: Option<usize>,
    /// Expected frame counter for loss detection
    expected_frame: u64,
}

impl Analyzer {
    /// Create a new analyzer with the given reference signal
    ///
    /// # Arguments
    /// * `reference` - The MLS sequence used for correlation
    /// * `sample_rate` - Sample rate in Hz for time calculations
    pub fn new(reference: &[f32], sample_rate: u32) -> Self {
        let ref_len = reference.len();
        // FFT size must be power of 2 and at least 2x reference length
        let fft_size = (ref_len * 2).next_power_of_two();

        let mut fft_planner = FftPlanner::new();

        // Pre-compute FFT of reference signal (zero-padded)
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

        Self {
            sample_rate,
            reference: reference.to_vec(),
            fft_planner,
            reference_fft: reference_complex,
            fft_size,
            last_latency: None,
            expected_frame: 0,
        }
    }

    /// Analyze received audio buffer
    ///
    /// # Arguments
    /// * `received` - Buffer of received audio samples
    ///
    /// # Returns
    /// Analysis result with latency and loss information
    pub fn analyze(&mut self, received: &[f32]) -> AnalysisResult {
        if received.len() < self.reference.len() {
            return AnalysisResult {
                is_healthy: false,
                ..Default::default()
            };
        }

        // Perform cross-correlation via FFT
        let (latency_samples, confidence) = self.cross_correlate(received);

        // Convert to milliseconds
        let latency_ms = (latency_samples as f64 / self.sample_rate as f64) * 1000.0;

        // Track latency changes for loss detection
        let lost_samples = self.detect_loss(latency_samples);

        // Determine health status
        let is_healthy = confidence > 0.5 && lost_samples == 0;

        self.last_latency = Some(latency_samples);

        AnalysisResult {
            latency_samples,
            latency_ms,
            confidence,
            lost_samples,
            corrupted_samples: 0, // TODO: Implement corruption detection
            is_healthy,
        }
    }

    /// Perform FFT-based cross-correlation
    ///
    /// The correlation peak search is constrained to a reasonable latency range
    /// (0-100ms) to prevent aliasing artifacts from the circular FFT correlation.
    /// This fixes the issue where latency would cycle through 0→341→682ms due to
    /// the MLS signal being periodic and the FFT producing aliased correlation peaks.
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
        // At any sample rate, 100ms = sample_rate / 10 samples
        // This prevents FFT circular correlation aliasing from causing latency
        // to cycle through MLS period multiples (0→341→682ms at 96kHz)
        let max_latency_samples = (self.sample_rate / 10) as usize;
        let search_limit = max_latency_samples.min(received_complex.len());

        // Find peak in correlation (constrained to latency bounds)
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

        // Normalize confidence by reference energy
        let ref_energy: f32 = self.reference.iter().map(|x| x * x).sum();
        let confidence = max_val / ref_energy.sqrt();

        (max_idx, confidence.min(1.0))
    }

    /// Detect sample loss based on latency changes (legacy heuristic)
    ///
    /// Note: This method is deprecated in favor of `detect_frame_loss` which
    /// uses the counter channel for accurate frame sequence tracking.
    fn detect_loss(&mut self, current_latency: usize) -> usize {
        match self.last_latency {
            Some(last) if current_latency > last => {
                // Latency increased - might indicate lost samples
                let diff = current_latency - last;
                if diff > 10 {
                    // Only report if significant
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
    /// The counter channel contains a sawtooth waveform (0.0 to 1.0) that
    /// encodes a 16-bit frame counter. By tracking the sequence, we can
    /// detect gaps indicating lost samples.
    ///
    /// This method provides accurate loss detection without the aliasing
    /// artifacts that affect the latency-delta heuristic.
    ///
    /// # Arguments
    /// * `counter_samples` - Samples from the counter channel (ch1)
    ///
    /// # Returns
    /// Number of frames lost (gaps in the counter sequence)
    pub fn detect_frame_loss(&mut self, counter_samples: &[f32]) -> usize {
        if counter_samples.is_empty() {
            return 0;
        }

        let mut total_lost = 0usize;

        for &sample in counter_samples {
            // Decode counter from normalized audio (0.0-1.0 → 0-65535)
            // Clamp to valid range in case of noise
            let normalized = sample.clamp(0.0, 1.0);
            let received_counter = (normalized * 65536.0) as u32 & 0xFFFF;

            if self.expected_frame > 0 {
                let expected = (self.expected_frame & 0xFFFF) as u32;

                // Calculate difference accounting for wrap-around
                let diff = if received_counter >= expected {
                    received_counter - expected
                } else {
                    // Wrap-around case
                    (65536 + received_counter as u64 - expected as u64) as u32
                };

                // If diff > 1 and < 32768 (half range), we have a gap
                // Large diffs (>32768) indicate backward movement, likely noise
                if diff > 1 && diff < 32768 {
                    total_lost += (diff - 1) as usize;
                }
            }

            // Update expected for next sample (will be current + 1)
            self.expected_frame = (received_counter as u64).wrapping_add(1);
        }

        total_lost
    }

    /// Get the configured sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Reset the analyzer state
    pub fn reset(&mut self) {
        self.last_latency = None;
        self.expected_frame = 0;
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
    fn test_latency_ms_calculation() {
        let gen = MlsGenerator::new(10);
        let sequence = gen.sequence().to_vec();

        let mut analyzer = Analyzer::new(&sequence, 48000);

        // 480 samples = 10ms at 48kHz
        let delay = 480;
        let mut delayed: Vec<f32> = vec![0.0; delay];
        delayed.extend(&sequence);

        let result = analyzer.analyze(&delayed);
        assert!((result.latency_ms - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_insufficient_samples() {
        let gen = MlsGenerator::new(10);
        let sequence = gen.sequence().to_vec();

        let mut analyzer = Analyzer::new(&sequence, 48000);

        // Too few samples
        let short_buffer = [0.0f32; 100];
        let result = analyzer.analyze(&short_buffer);
        assert!(!result.is_healthy);
    }
}
