//! Burst signal generation for frame-based latency measurement
//!
//! Generates a 10ms burst of white noise every 100ms, enabling precise
//! latency measurement through frame counter comparison.
//! This approach measures latency via sample counting rather than wall-clock
//! timestamps, eliminating ring buffer accumulation delays.

/// Duration of silence before burst (90ms of 100ms cycle)
const SILENCE_RATIO: f32 = 0.9;

/// Burst amplitude (-6dB for headroom)
const BURST_AMPLITUDE: f32 = 0.5;

/// Event emitted when a burst starts in the output callback
#[derive(Debug, Clone)]
pub struct BurstEvent {
    /// Output frame counter at burst start (the authoritative timing reference)
    pub start_frame: u64,
}

/// Event emitted when a burst is detected in the input callback
#[derive(Debug, Clone)]
pub struct DetectionEvent {
    /// Input frame counter at burst detection
    pub input_frame: u64,
}

/// Burst signal generator for latency measurement
///
/// Generates a 10ms burst of white noise every 100ms cycle.
/// The burst timing is captured via [`BurstEvent`] for timestamp-based
/// latency calculation.
///
/// # Example
/// ```
/// use audiotester_core::audio::burst::BurstGenerator;
///
/// let mut gen = BurstGenerator::new(96000);
/// let (sample, is_burst_start) = gen.next_sample();
/// ```
#[derive(Debug)]
pub struct BurstGenerator {
    /// Sample rate in Hz
    sample_rate: u32,
    /// Total cycle length in samples (100ms)
    cycle_length: usize,
    /// Position where burst starts (90ms into cycle)
    burst_start_position: usize,
    /// Current position in cycle (0..cycle_length)
    cycle_position: usize,
    /// PRNG state for noise generation
    noise_seed: u32,
    /// Amplitude scaling factor
    amplitude: f32,
}

impl BurstGenerator {
    /// Create a new burst generator
    ///
    /// # Arguments
    /// * `sample_rate` - Sample rate in Hz (e.g., 96000)
    ///
    /// # Example
    /// ```
    /// use audiotester_core::audio::burst::BurstGenerator;
    ///
    /// let gen = BurstGenerator::new(96000);
    /// assert_eq!(gen.cycle_length(), 9600); // 100ms at 96kHz
    /// ```
    pub fn new(sample_rate: u32) -> Self {
        let cycle_length = (sample_rate as f64 * 0.1) as usize; // 100ms
        let burst_start_position = (cycle_length as f32 * SILENCE_RATIO) as usize;

        Self {
            sample_rate,
            cycle_length,
            burst_start_position,
            cycle_position: 0,
            noise_seed: 0xDEADBEEF,
            amplitude: BURST_AMPLITUDE,
        }
    }

    /// Get the next sample from the generator
    ///
    /// Returns a tuple of (sample, is_burst_start).
    /// - `sample` is 0.0 during silence, white noise during burst
    /// - `is_burst_start` is true only on the first sample of each burst
    ///
    /// # Example
    /// ```
    /// use audiotester_core::audio::burst::BurstGenerator;
    ///
    /// let mut gen = BurstGenerator::new(48000);
    /// // First samples are silence
    /// let (sample, is_start) = gen.next_sample();
    /// assert_eq!(sample, 0.0);
    /// assert!(!is_start);
    /// ```
    pub fn next_sample(&mut self) -> (f32, bool) {
        let is_burst_start = self.cycle_position == self.burst_start_position;
        let in_burst = self.cycle_position >= self.burst_start_position;

        let sample = if in_burst {
            self.generate_noise() * self.amplitude
        } else {
            0.0
        };

        self.cycle_position = (self.cycle_position + 1) % self.cycle_length;
        (sample, is_burst_start)
    }

    /// Generate a single noise sample using LCG PRNG
    ///
    /// Uses a linear congruential generator to produce pseudo-random
    /// white noise. The noise is band-limited in practice due to the
    /// sample rate, but provides good high-frequency content for
    /// envelope detection.
    fn generate_noise(&mut self) -> f32 {
        // LCG parameters (same as glibc)
        self.noise_seed = self.noise_seed.wrapping_mul(1103515245).wrapping_add(12345);
        // Convert to -1.0..1.0 range
        let bits = (self.noise_seed >> 16) & 0x7FFF;
        (bits as f32 / 16384.0) - 1.0
    }

    /// Fill a buffer with sequential samples
    ///
    /// Returns a vector of frame indices where bursts started within this buffer.
    ///
    /// # Arguments
    /// * `buffer` - Buffer to fill with samples
    ///
    /// # Returns
    /// Indices into the buffer where burst starts occurred
    pub fn fill_buffer(&mut self, buffer: &mut [f32]) -> Vec<usize> {
        let mut burst_starts = Vec::new();
        for (i, sample) in buffer.iter_mut().enumerate() {
            let (s, is_start) = self.next_sample();
            *sample = s;
            if is_start {
                burst_starts.push(i);
            }
        }
        burst_starts
    }

    /// Get cycle length in samples (100ms at configured sample rate)
    pub fn cycle_length(&self) -> usize {
        self.cycle_length
    }

    /// Get burst start position within cycle (90% into cycle)
    pub fn burst_start_position(&self) -> usize {
        self.burst_start_position
    }

    /// Get burst duration in samples (10ms at configured sample rate)
    pub fn burst_duration(&self) -> usize {
        self.cycle_length - self.burst_start_position
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get current position in cycle
    pub fn position(&self) -> usize {
        self.cycle_position
    }

    /// Check if currently in burst phase
    pub fn in_burst(&self) -> bool {
        self.cycle_position >= self.burst_start_position
    }

    /// Reset generator to start of cycle
    pub fn reset(&mut self) {
        self.cycle_position = 0;
        self.noise_seed = 0xDEADBEEF;
    }

    /// Set amplitude scaling factor
    ///
    /// # Arguments
    /// * `amplitude` - Amplitude from 0.0 to 1.0
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude.clamp(0.0, 1.0);
    }

    /// Get current amplitude
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }

    /// Get the burst update rate in Hz (10 measurements per second)
    pub fn update_rate(&self) -> f32 {
        self.sample_rate as f32 / self.cycle_length as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_length() {
        let gen = BurstGenerator::new(96000);
        assert_eq!(gen.cycle_length(), 9600); // 100ms at 96kHz

        let gen = BurstGenerator::new(48000);
        assert_eq!(gen.cycle_length(), 4800); // 100ms at 48kHz
    }

    #[test]
    fn test_burst_timing() {
        let gen = BurstGenerator::new(96000);
        // Burst starts at 90% of cycle
        assert_eq!(gen.burst_start_position(), 8640); // 90ms at 96kHz
                                                      // Burst lasts 10% of cycle
        assert_eq!(gen.burst_duration(), 960); // 10ms at 96kHz
    }

    #[test]
    fn test_silence_before_burst() {
        let mut gen = BurstGenerator::new(48000);

        // First 90% should be silence
        for i in 0..gen.burst_start_position() {
            let (sample, is_start) = gen.next_sample();
            assert_eq!(sample, 0.0, "Sample {} should be silence", i);
            assert!(!is_start, "Sample {} should not be burst start", i);
        }
    }

    #[test]
    fn test_burst_detection() {
        let mut gen = BurstGenerator::new(48000);

        // Advance to just before burst
        for _ in 0..gen.burst_start_position() {
            gen.next_sample();
        }

        // First burst sample
        let (sample, is_start) = gen.next_sample();
        assert!(is_start, "First burst sample should be start");
        assert!(sample != 0.0, "Burst sample should be non-zero");

        // Second burst sample
        let (_, is_start) = gen.next_sample();
        assert!(!is_start, "Second burst sample should not be start");
    }

    #[test]
    fn test_cycle_repeats() {
        let mut gen = BurstGenerator::new(48000);
        let cycle_len = gen.cycle_length();

        // Complete one full cycle
        let mut first_burst_start = None;
        for i in 0..cycle_len {
            let (_, is_start) = gen.next_sample();
            if is_start {
                first_burst_start = Some(i);
            }
        }

        // Second cycle should have burst at same position
        for i in 0..cycle_len {
            let (_, is_start) = gen.next_sample();
            if is_start {
                assert_eq!(Some(i), first_burst_start);
                break;
            }
        }
    }

    #[test]
    fn test_fill_buffer() {
        let mut gen = BurstGenerator::new(48000);
        let mut buffer = vec![0.0f32; gen.cycle_length()];

        let burst_starts = gen.fill_buffer(&mut buffer);

        // Should have exactly one burst start per cycle
        assert_eq!(burst_starts.len(), 1);
        assert_eq!(burst_starts[0], gen.burst_start_position());

        // Verify buffer contents
        for (i, &sample) in buffer.iter().enumerate() {
            if i < gen.burst_start_position() {
                assert_eq!(sample, 0.0, "Pre-burst should be silence");
            } else {
                assert!(
                    sample != 0.0 || i > gen.burst_start_position(),
                    "Burst should be non-zero (some samples may be 0 by chance)"
                );
            }
        }
    }

    #[test]
    fn test_noise_distribution() {
        let mut gen = BurstGenerator::new(48000);

        // Advance to burst
        for _ in 0..gen.burst_start_position() {
            gen.next_sample();
        }

        // Collect burst samples
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        let mut sum = 0.0f32;
        let n = 1000;

        for _ in 0..n {
            let noise = gen.generate_noise();
            min = min.min(noise);
            max = max.max(noise);
            sum += noise;
        }

        let mean = sum / n as f32;

        // Noise should span most of -1..1 range
        assert!(min < -0.8, "Min noise should be < -0.8, got {}", min);
        assert!(max > 0.8, "Max noise should be > 0.8, got {}", max);
        // Mean should be close to 0
        assert!(
            mean.abs() < 0.1,
            "Mean noise should be near 0, got {}",
            mean
        );
    }

    #[test]
    fn test_update_rate() {
        let gen = BurstGenerator::new(96000);
        assert!((gen.update_rate() - 10.0).abs() < 0.01); // 10 Hz
    }

    #[test]
    fn test_reset() {
        let mut gen = BurstGenerator::new(48000);

        // Advance some samples
        for _ in 0..1000 {
            gen.next_sample();
        }

        gen.reset();
        assert_eq!(gen.position(), 0);
    }

    #[test]
    fn test_amplitude() {
        let mut gen = BurstGenerator::new(48000);
        gen.set_amplitude(0.25);

        // Advance to burst
        for _ in 0..gen.burst_start_position() {
            gen.next_sample();
        }

        // Burst samples should be limited by amplitude
        for _ in 0..100 {
            let (sample, _) = gen.next_sample();
            assert!(sample.abs() <= 0.25, "Sample {} exceeds amplitude", sample);
        }
    }
}
