//! MLS (Maximum Length Sequence) test signal generation
//!
//! Generates pseudo-random binary sequences with perfect autocorrelation
//! properties, ideal for latency measurement and sample identification.

/// MLS (Maximum Length Sequence) generator
///
/// Generates a pseudo-random binary sequence of length 2^order - 1 with
/// perfect autocorrelation properties. The sequence can be used for:
/// - Precise latency measurement via cross-correlation
/// - Sample-accurate position identification
/// - Robust detection in noisy environments
#[derive(Debug, Clone)]
pub struct MlsGenerator {
    /// Order of the sequence (sequence length = 2^order - 1)
    order: u32,
    /// Length of the sequence
    length: usize,
    /// Current position in the sequence
    position: usize,
    /// Pre-generated sequence buffer
    sequence: Vec<f32>,
    /// Amplitude scaling factor
    amplitude: f32,
}

impl MlsGenerator {
    /// Create a new MLS generator with the specified order
    ///
    /// # Arguments
    /// * `order` - Order of the sequence (2-15). Sequence length = 2^order - 1
    ///
    /// # Panics
    /// Panics if order is less than 2 or greater than 15
    ///
    /// # Example
    /// ```
    /// use audiotester::audio::signal::MlsGenerator;
    ///
    /// let mut gen = MlsGenerator::new(10); // 1023 sample sequence
    /// let sample = gen.next_sample();
    /// ```
    pub fn new(order: u32) -> Self {
        assert!((2..=15).contains(&order), "Order must be between 2 and 15");

        let length = (1usize << order) - 1;
        let sequence = Self::generate_sequence(order, length);

        Self {
            order,
            length,
            position: 0,
            sequence,
            amplitude: 0.5, // -6dB to leave headroom
        }
    }

    /// Generate the MLS sequence using LFSR with proper primitive polynomials
    fn generate_sequence(order: u32, length: usize) -> Vec<f32> {
        let mut sequence = Vec::with_capacity(length);
        let mut lfsr: u32 = 1; // Initial state (non-zero)

        // Tap positions for maximal length sequences (Fibonacci LFSR)
        // These are the bit positions to XOR for feedback (0-indexed)
        // Source: https://en.wikipedia.org/wiki/Linear-feedback_shift_register
        let taps: &[u32] = match order {
            2 => &[1, 0],
            3 => &[2, 1],
            4 => &[3, 2],
            5 => &[4, 2],
            6 => &[5, 4],
            7 => &[6, 5],
            8 => &[7, 5, 4, 3],
            9 => &[8, 4],
            10 => &[9, 6],
            11 => &[10, 8],
            12 => &[11, 10, 9, 3],
            13 => &[12, 11, 10, 7],
            14 => &[13, 12, 11, 1],
            15 => &[14, 13],
            _ => &[1, 0], // Fallback
        };

        for _ in 0..length {
            // Output is the LSB
            let bit = lfsr & 1;
            sequence.push(if bit == 1 { 1.0 } else { -1.0 });

            // Calculate feedback by XORing all tap positions
            let mut feedback = 0u32;
            for &tap in taps {
                feedback ^= (lfsr >> tap) & 1;
            }

            // Shift right and insert feedback at MSB
            lfsr = (lfsr >> 1) | (feedback << (order - 1));
        }

        sequence
    }

    /// Get the next sample from the sequence
    ///
    /// The sequence repeats continuously after reaching the end.
    pub fn next_sample(&mut self) -> f32 {
        let sample = self.sequence[self.position] * self.amplitude;
        self.position = (self.position + 1) % self.length;
        sample
    }

    /// Fill a buffer with sequential samples
    ///
    /// # Arguments
    /// * `buffer` - Buffer to fill with samples
    pub fn fill_buffer(&mut self, buffer: &mut [f32]) {
        for sample in buffer.iter_mut() {
            *sample = self.next_sample();
        }
    }

    /// Get current position in the sequence
    pub fn position(&self) -> usize {
        self.position
    }

    /// Reset the generator to the start of the sequence
    pub fn reset(&mut self) {
        self.position = 0;
    }

    /// Get the sequence length
    pub fn length(&self) -> usize {
        self.length
    }

    /// Get the order (sequence length = 2^order - 1)
    pub fn order(&self) -> u32 {
        self.order
    }

    /// Get the full sequence for correlation
    pub fn sequence(&self) -> &[f32] {
        &self.sequence
    }

    /// Set the amplitude scaling factor
    ///
    /// # Arguments
    /// * `amplitude` - Amplitude from 0.0 to 1.0
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude.clamp(0.0, 1.0);
    }

    /// Get the current amplitude
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }
}

impl Default for MlsGenerator {
    fn default() -> Self {
        Self::new(crate::MLS_ORDER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mls_length() {
        let gen = MlsGenerator::new(10);
        assert_eq!(gen.length(), 1023);

        let gen = MlsGenerator::new(15);
        assert_eq!(gen.length(), 32767);
    }

    #[test]
    fn test_mls_values_are_bipolar() {
        let gen = MlsGenerator::new(10);
        for &sample in gen.sequence() {
            assert!(sample == 1.0 || sample == -1.0);
        }
    }

    #[test]
    fn test_mls_repeats() {
        let mut gen = MlsGenerator::new(5); // Short sequence for testing
        let length = gen.length();

        let first_samples: Vec<f32> = (0..length).map(|_| gen.next_sample()).collect();
        let second_samples: Vec<f32> = (0..length).map(|_| gen.next_sample()).collect();

        assert_eq!(first_samples, second_samples);
    }

    #[test]
    fn test_mls_reset() {
        let mut gen = MlsGenerator::new(10);

        // Advance some samples
        for _ in 0..100 {
            gen.next_sample();
        }
        assert_eq!(gen.position(), 100);

        gen.reset();
        assert_eq!(gen.position(), 0);
    }

    #[test]
    fn test_mls_amplitude() {
        let mut gen = MlsGenerator::new(10);
        gen.set_amplitude(0.25);

        for _ in 0..100 {
            let sample = gen.next_sample();
            assert!(sample.abs() <= 0.25);
        }
    }

    #[test]
    fn test_mls_fill_buffer() {
        let mut gen = MlsGenerator::new(10);
        let mut buffer = [0.0f32; 64];

        gen.fill_buffer(&mut buffer);

        // All samples should be non-zero
        for &sample in &buffer {
            assert!(sample != 0.0);
        }
    }

    #[test]
    #[should_panic]
    fn test_mls_order_too_low() {
        MlsGenerator::new(1);
    }

    #[test]
    #[should_panic]
    fn test_mls_order_too_high() {
        MlsGenerator::new(16);
    }
}
