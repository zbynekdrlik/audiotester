//! E2E tests for MLS signal generation
//!
//! Verifies the mathematical properties of MLS sequences that are
//! essential for accurate latency measurement and sample identification.

use audiotester::audio::signal::MlsGenerator;

/// Test that MLS has the correct length for given order
#[test]
fn test_mls_length_property() {
    for order in 2..=15 {
        let gen = MlsGenerator::new(order);
        let expected_length = (1usize << order) - 1;
        assert_eq!(
            gen.length(),
            expected_length,
            "MLS order {} should have length {}",
            order,
            expected_length
        );
    }
}

/// Test that MLS values are bipolar (+1 or -1 before amplitude scaling)
#[test]
fn test_mls_bipolar_values() {
    let gen = MlsGenerator::new(15);
    let sequence = gen.sequence();

    for (i, &sample) in sequence.iter().enumerate() {
        assert!(
            sample == 1.0 || sample == -1.0,
            "Sample {} should be +1 or -1, got {}",
            i,
            sample
        );
    }
}

/// Test that MLS is balanced (roughly equal +1 and -1 counts)
#[test]
fn test_mls_balance() {
    let gen = MlsGenerator::new(15);
    let sequence = gen.sequence();

    let positive_count = sequence.iter().filter(|&&x| x > 0.0).count();
    let negative_count = sequence.iter().filter(|&&x| x < 0.0).count();

    // For MLS of order n, there are 2^(n-1) ones and 2^(n-1)-1 zeros
    // So positive and negative should differ by exactly 1
    let diff = (positive_count as i64 - negative_count as i64).abs();
    assert_eq!(diff, 1, "MLS should be balanced (diff of 1)");
}

/// Test that MLS repeats correctly
#[test]
fn test_mls_periodicity() {
    let mut gen = MlsGenerator::new(10);
    let length = gen.length();

    // Collect two full periods
    let period1: Vec<f32> = (0..length).map(|_| gen.next_sample()).collect();
    let period2: Vec<f32> = (0..length).map(|_| gen.next_sample()).collect();

    // Normalize for comparison (remove amplitude)
    let norm1: Vec<i32> = period1
        .iter()
        .map(|&x| if x > 0.0 { 1 } else { -1 })
        .collect();
    let norm2: Vec<i32> = period2
        .iter()
        .map(|&x| if x > 0.0 { 1 } else { -1 })
        .collect();

    assert_eq!(norm1, norm2, "MLS should repeat exactly");
}

/// Test MLS autocorrelation has sharp peak (impulse-like)
#[test]
fn test_mls_autocorrelation() {
    let gen = MlsGenerator::new(10);
    let sequence = gen.sequence();
    let n = sequence.len();

    // Calculate autocorrelation at different lags
    let mut autocorr = Vec::with_capacity(n);

    for lag in 0..n {
        let mut sum = 0.0f32;
        for i in 0..n {
            let j = (i + lag) % n;
            sum += sequence[i] * sequence[j];
        }
        autocorr.push(sum);
    }

    // Peak should be at lag 0
    let peak_value = autocorr[0];
    assert_eq!(
        peak_value as i32, n as i32,
        "Autocorrelation peak should equal sequence length"
    );

    // All other values should be approximately -1 (for MLS)
    for (lag, &value) in autocorr.iter().enumerate().skip(1) {
        assert!(
            (value - (-1.0)).abs() < 1.5,
            "Autocorrelation at lag {} should be ~-1, got {}",
            lag,
            value
        );
    }
}

/// Test that MLS contains all non-zero states (full period)
#[test]
fn test_mls_full_period() {
    let gen = MlsGenerator::new(8);
    let sequence = gen.sequence();

    // Convert to bit patterns and check uniqueness
    let mut patterns: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Slide a window across the sequence
    for i in 0..sequence.len() {
        let mut pattern = String::new();
        for j in 0..8 {
            let idx = (i + j) % sequence.len();
            pattern.push(if sequence[idx] > 0.0 { '1' } else { '0' });
        }
        patterns.insert(pattern);
    }

    // Should have 2^8 - 1 unique patterns (all except all-zeros)
    assert_eq!(
        patterns.len(),
        255,
        "MLS order 8 should contain 255 unique 8-bit patterns"
    );
}

/// Test amplitude scaling
#[test]
fn test_mls_amplitude_scaling() {
    let mut gen = MlsGenerator::new(10);

    // Default amplitude
    let default_amp = gen.amplitude();
    assert!(default_amp > 0.0 && default_amp <= 1.0);

    // Test custom amplitude
    gen.set_amplitude(0.25);
    for _ in 0..100 {
        let sample = gen.next_sample();
        assert!(
            sample.abs() <= 0.25 + 0.001,
            "Sample {} exceeds amplitude 0.25",
            sample
        );
    }

    // Test amplitude clamping
    gen.set_amplitude(2.0);
    assert_eq!(gen.amplitude(), 1.0, "Amplitude should clamp to 1.0");

    gen.set_amplitude(-1.0);
    assert_eq!(gen.amplitude(), 0.0, "Amplitude should clamp to 0.0");
}

/// Test reset functionality
#[test]
fn test_mls_reset() {
    let mut gen = MlsGenerator::new(10);

    // Get first 100 samples
    let first_run: Vec<f32> = (0..100).map(|_| gen.next_sample()).collect();

    // Advance further
    for _ in 0..500 {
        gen.next_sample();
    }

    // Reset and get samples again
    gen.reset();
    let second_run: Vec<f32> = (0..100).map(|_| gen.next_sample()).collect();

    assert_eq!(first_run, second_run, "Reset should return to start");
}

/// Test fill_buffer produces consecutive samples
#[test]
fn test_mls_fill_buffer() {
    let mut gen1 = MlsGenerator::new(10);
    let mut gen2 = MlsGenerator::new(10);

    // Fill buffer with gen1
    let mut buffer = [0.0f32; 256];
    gen1.fill_buffer(&mut buffer);

    // Get samples one by one with gen2
    let individual: Vec<f32> = (0..256).map(|_| gen2.next_sample()).collect();

    assert_eq!(
        buffer.to_vec(),
        individual,
        "fill_buffer should produce same results as individual next_sample calls"
    );
}

/// Test position tracking
#[test]
fn test_mls_position() {
    let mut gen = MlsGenerator::new(10);
    let length = gen.length(); // 1023

    assert_eq!(gen.position(), 0, "Initial position should be 0");

    for i in 1..100 {
        gen.next_sample();
        assert_eq!(gen.position(), i, "Position should track samples");
    }
    // After loop: position = 99

    // Test wraparound: advance to complete one full period + 1
    // Currently at 99, need (length - 99) more to reach end, then 1 more to wrap
    for _ in 0..(length - 99 + 1) {
        gen.next_sample();
    }
    // After 99 + (1023 - 99 + 1) = 1024 samples, position = 1024 % 1023 = 1
    assert_eq!(gen.position(), 1, "Position should wrap around");
}
