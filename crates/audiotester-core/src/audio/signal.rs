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
    /// use audiotester_core::audio::signal::MlsGenerator;
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

    /// Generate the MLS sequence using Galois LFSR
    fn generate_sequence(order: u32, length: usize) -> Vec<f32> {
        let mut sequence = Vec::with_capacity(length);
        let mut lfsr: u32 = 1; // Initial state (non-zero)

        // Feedback masks for Galois LFSR (primitive polynomials).
        // In a Galois LFSR, the mask represents XOR tap positions applied when
        // the output bit (LSB) is 1. Each mask encodes a maximal-length polynomial
        // that produces a sequence of 2^order - 1 unique states before repeating.
        // Source: https://docs.xilinx.com/v/u/en-US/xapp052 (Xilinx LFSR reference)
        let mask: u32 = match order {
            2 => 0x3,     // x^2 + x + 1
            3 => 0x6,     // x^3 + x^2 + 1
            4 => 0xC,     // x^4 + x^3 + 1
            5 => 0x14,    // x^5 + x^3 + 1
            6 => 0x30,    // x^6 + x^5 + 1
            7 => 0x60,    // x^7 + x^6 + 1
            8 => 0xB8,    // x^8 + x^6 + x^5 + x^4 + 1
            9 => 0x110,   // x^9 + x^5 + 1
            10 => 0x240,  // x^10 + x^7 + 1
            11 => 0x500,  // x^11 + x^9 + 1
            12 => 0xE08,  // x^12 + x^11 + x^10 + x^4 + 1
            13 => 0x1C80, // x^13 + x^12 + x^11 + x^8 + 1
            14 => 0x3802, // x^14 + x^13 + x^12 + x^2 + 1
            15 => 0x6000, // x^15 + x^14 + 1
            _ => 0x3,     // Fallback to order 2
        };

        for _ in 0..length {
            // Output is the LSB
            let output = lfsr & 1;
            sequence.push(if output == 1 { 1.0 } else { -1.0 });

            // Galois LFSR: shift right, XOR with mask if output was 1
            lfsr >>= 1;
            if output == 1 {
                lfsr ^= mask;
            }
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
