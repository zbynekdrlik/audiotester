//! E2E tests for latency measurement
//!
//! Verifies that the cross-correlation based latency detection
//! produces accurate results across various scenarios.

use audiotester::audio::{analyzer::Analyzer, signal::MlsGenerator};

/// Test latency detection with no delay
#[test]
fn test_zero_latency() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let result = analyzer.analyze(&sequence);

    assert_eq!(
        result.latency_samples, 0,
        "Zero delay should give zero latency"
    );
    assert!(
        result.confidence > 0.9,
        "Perfect signal should have high confidence, got {}",
        result.confidence
    );
    assert!(result.is_healthy, "Perfect signal should be healthy");
}

/// Test latency detection with various delays
#[test]
fn test_known_delays() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();

    let test_delays = [1, 10, 100, 480, 1000, 2000];

    for &delay in &test_delays {
        let mut analyzer = Analyzer::new(&sequence, 48000);

        // Create delayed signal
        let mut delayed = vec![0.0f32; delay];
        delayed.extend(&sequence);

        let result = analyzer.analyze(&delayed);

        assert_eq!(
            result.latency_samples, delay,
            "Delay of {} samples should be detected correctly",
            delay
        );
        assert!(
            result.confidence > 0.5,
            "Delayed signal should still have reasonable confidence"
        );
    }
}

/// Test latency in milliseconds calculation
#[test]
fn test_latency_ms_calculation() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();

    let sample_rate = 48000;
    let mut analyzer = Analyzer::new(&sequence, sample_rate);

    // 480 samples at 48kHz = 10ms
    let delay = 480;
    let mut delayed = vec![0.0f32; delay];
    delayed.extend(&sequence);

    let result = analyzer.analyze(&delayed);

    let expected_ms = 10.0;
    assert!(
        (result.latency_ms - expected_ms).abs() < 0.1,
        "Expected ~{}ms, got {}ms",
        expected_ms,
        result.latency_ms
    );
}

/// Test different sample rates
#[test]
fn test_sample_rate_independence() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();

    let sample_rates = [44100, 48000, 96000];
    let target_ms = 10.0;

    for &sr in &sample_rates {
        let mut analyzer = Analyzer::new(&sequence, sr);

        // Calculate samples for target milliseconds
        let delay_samples = ((target_ms / 1000.0) * sr as f64) as usize;
        let mut delayed = vec![0.0f32; delay_samples];
        delayed.extend(&sequence);

        let result = analyzer.analyze(&delayed);

        assert!(
            (result.latency_ms - target_ms).abs() < 0.5,
            "At {}Hz, {}ms delay should be detected (got {}ms)",
            sr,
            target_ms,
            result.latency_ms
        );
    }
}

/// Test detection with noise
#[test]
fn test_noise_tolerance() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 100;

    // Add gaussian-ish noise (simple pseudo-random)
    let mut noisy: Vec<f32> = vec![0.0f32; delay];
    noisy.extend(&sequence);

    let noise_level = 0.1;
    let mut seed: u32 = 12345;
    for sample in &mut noisy {
        // Simple LCG for pseudo-random noise
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let noise = ((seed >> 16) as f32 / 32768.0 - 1.0) * noise_level;
        *sample += noise;
    }

    let result = analyzer.analyze(&noisy);

    assert_eq!(
        result.latency_samples, delay,
        "Should detect correct latency despite noise"
    );
    assert!(
        result.confidence > 0.3,
        "Noisy signal should still have some confidence"
    );
}

/// Test detection with amplitude variations
#[test]
fn test_amplitude_invariance() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 200;
    let amplitudes = [0.1, 0.5, 1.0, 2.0];

    for &amp in &amplitudes {
        analyzer.reset();

        let mut scaled: Vec<f32> = vec![0.0f32; delay];
        scaled.extend(sequence.iter().map(|&x| x * amp));

        let result = analyzer.analyze(&scaled);

        assert_eq!(
            result.latency_samples, delay,
            "Amplitude {} should not affect latency detection",
            amp
        );
    }
}

/// Test with inverted signal (phase flip)
#[test]
fn test_inverted_signal() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 150;
    let mut inverted: Vec<f32> = vec![0.0f32; delay];
    inverted.extend(sequence.iter().map(|&x| -x));

    let result = analyzer.analyze(&inverted);

    // Inverted signal should still correlate (we detect magnitude)
    assert_eq!(
        result.latency_samples, delay,
        "Inverted signal should still be detected"
    );
}

/// Test insufficient buffer handling
#[test]
fn test_insufficient_buffer() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    // Buffer smaller than reference
    let short_buffer = [0.0f32; 100];
    let result = analyzer.analyze(&short_buffer);

    assert!(
        !result.is_healthy,
        "Insufficient buffer should report unhealthy"
    );
}

/// Test reset functionality
#[test]
fn test_analyzer_reset() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    // First measurement
    let mut delayed = vec![0.0f32; 100];
    delayed.extend(&sequence);
    let result1 = analyzer.analyze(&delayed);

    // Reset
    analyzer.reset();

    // Same measurement should give same result
    let result2 = analyzer.analyze(&delayed);

    assert_eq!(
        result1.latency_samples, result2.latency_samples,
        "Reset analyzer should produce consistent results"
    );
}

/// Test sub-sample accuracy is not claimed
#[test]
fn test_integer_sample_latency() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 123;
    let mut delayed = vec![0.0f32; delay];
    delayed.extend(&sequence);

    let result = analyzer.analyze(&delayed);

    // Result should be exact integer samples
    assert_eq!(
        result.latency_samples, delay,
        "Latency should be exact integer samples"
    );
}

/// Test continuous monitoring (multiple sequential analyses)
#[test]
fn test_continuous_monitoring() {
    let gen = MlsGenerator::new(10); // Shorter for faster test
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 50;
    let mut delayed = vec![0.0f32; delay];
    delayed.extend(&sequence);

    // Simulate continuous monitoring
    for iteration in 0..10 {
        let result = analyzer.analyze(&delayed);
        assert_eq!(
            result.latency_samples, delay,
            "Iteration {}: latency should be consistent",
            iteration
        );
    }
}
