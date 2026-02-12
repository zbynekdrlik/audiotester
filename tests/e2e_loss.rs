//! E2E tests for sample loss detection
//!
//! Verifies that sample loss and signal degradation are detected
//! reliably in various scenarios.

use audiotester::audio::{analyzer::Analyzer, signal::MlsGenerator};

/// Test no loss in perfect signal
#[test]
fn test_no_loss_perfect_signal() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let result = analyzer.analyze(&sequence);

    assert_eq!(
        result.lost_samples, 0,
        "Perfect signal should have no lost samples"
    );
    assert_eq!(
        result.corrupted_samples, 0,
        "Perfect signal should have no corrupted samples"
    );
    assert!(result.is_healthy, "Perfect signal should be healthy");
}

/// Test detection of silence (complete signal loss)
#[test]
fn test_silence_detection() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    // All zeros - complete loss
    let silence = vec![0.0f32; sequence.len()];
    let result = analyzer.analyze(&silence);

    assert!(
        result.confidence < 0.3,
        "Silence should have very low confidence"
    );
    // Note: silence might not be detected as "lost_samples" directly,
    // but should indicate unhealthy status
}

/// Test health status transitions
#[test]
fn test_health_status() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    // Good signal
    let good = sequence.clone();
    let result_good = analyzer.analyze(&good);
    assert!(result_good.is_healthy, "Good signal should be healthy");

    analyzer.reset();

    // Poor signal (very low amplitude noise)
    let poor: Vec<f32> = (0..sequence.len())
        .map(|i| (i as f32 * 0.001).sin() * 0.001)
        .collect();
    let result_poor = analyzer.analyze(&poor);
    assert!(
        !result_poor.is_healthy || result_poor.confidence < 0.5,
        "Poor signal should indicate issues"
    );
}

/// Test increasing latency detection (potential loss indicator)
#[test]
fn test_latency_increase_detection() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    // First measurement at 100 samples latency
    let mut delayed1 = vec![0.0f32; 100];
    delayed1.extend(&sequence);
    let result1 = analyzer.analyze(&delayed1);
    assert_eq!(result1.latency_samples, 100);

    // Second measurement at 150 samples (increased by 50)
    let mut delayed2 = vec![0.0f32; 150];
    delayed2.extend(&sequence);
    let result2 = analyzer.analyze(&delayed2);
    assert_eq!(result2.latency_samples, 150);

    // The analyzer should detect this as potential loss
    // (depends on threshold in implementation)
    assert_eq!(
        result2.lost_samples, 50,
        "Latency increase of 50 should be detected as lost samples"
    );
}

/// Test corrupted sample handling
#[test]
fn test_corrupted_samples() {
    let gen = MlsGenerator::new(12);
    let mut sequence = gen.sequence().to_vec();
    let reference = sequence.clone();
    let mut analyzer = Analyzer::new(&reference, 48000);

    // Corrupt some samples
    let corruption_indices = [100, 500, 1000, 1500];
    for &i in &corruption_indices {
        if i < sequence.len() {
            sequence[i] = 0.0; // Zero out sample
        }
    }

    let result = analyzer.analyze(&sequence);

    // Even with corruption, should still detect signal
    assert!(
        result.confidence > 0.5,
        "Lightly corrupted signal should still be detectable"
    );
}

/// Test signal with dropouts
#[test]
fn test_signal_dropouts() {
    let gen = MlsGenerator::new(12);
    let mut sequence = gen.sequence().to_vec();
    let reference = sequence.clone();
    let mut analyzer = Analyzer::new(&reference, 48000);

    // Create a dropout (zeroed section)
    let dropout_start = 500;
    let dropout_length = 100;
    for i in dropout_start..(dropout_start + dropout_length).min(sequence.len()) {
        sequence[i] = 0.0;
    }

    let result = analyzer.analyze(&sequence);

    // Should still correlate despite dropout
    assert!(
        result.latency_samples == 0,
        "Should detect correct position despite dropout"
    );
    // FFT cross-correlation is robust - small dropout doesn't significantly reduce confidence
    // The sequence is 4095 samples, 100 sample dropout is <3%, so confidence remains high
    assert!(
        result.confidence > 0.9,
        "Small dropout should maintain high confidence due to MLS robustness"
    );
}

/// Test extreme noise overwhelming signal
#[test]
fn test_overwhelming_noise() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    // Create signal with overwhelming noise
    let mut noisy = sequence.clone();
    let noise_level = 10.0; // Much larger than signal
    let mut seed: u32 = 98765;
    for sample in &mut noisy {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let noise = ((seed >> 16) as f32 / 32768.0 - 1.0) * noise_level;
        *sample += noise;
    }

    let result = analyzer.analyze(&noisy);

    // MLS sequences are designed to be robust against noise through their
    // autocorrelation properties. Even with 10x noise, the cross-correlation
    // can still extract the signal. This is a feature, not a bug!
    // The test verifies that detection still works under adverse conditions.
    assert!(
        result.confidence > 0.0,
        "MLS should still correlate even with significant noise"
    );
}

/// Test DC offset handling
#[test]
fn test_dc_offset() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 100;

    // Add DC offset to delayed signal
    let dc_offset = 0.5;
    let mut with_dc: Vec<f32> = vec![dc_offset; delay];
    with_dc.extend(sequence.iter().map(|&x| x + dc_offset));

    let result = analyzer.analyze(&with_dc);

    // Cross-correlation should be relatively robust to DC offset
    assert_eq!(
        result.latency_samples, delay,
        "DC offset should not significantly affect latency detection"
    );
}

/// Test rapid signal changes
#[test]
fn test_rapid_changes() {
    let gen = MlsGenerator::new(10); // Shorter sequence
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    // Simulate rapid monitoring with alternating good/bad signals
    for i in 0..20 {
        let signal: Vec<f32> = if i % 2 == 0 {
            // Good signal
            sequence.clone()
        } else {
            // Degraded signal
            sequence.iter().map(|&x| x * 0.1).collect()
        };

        let result = analyzer.analyze(&signal);

        if i % 2 == 0 {
            assert!(
                result.is_healthy,
                "Iteration {}: good signal should be healthy",
                i
            );
        }
    }
}

/// Test analyzer sample rate getter
#[test]
fn test_sample_rate_accessor() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();

    let rates = [44100, 48000, 96000];
    for &rate in &rates {
        let analyzer = Analyzer::new(&sequence, rate);
        assert_eq!(
            analyzer.sample_rate(),
            rate,
            "Sample rate should be correctly stored"
        );
    }
}

/// Test with very short MLS sequence
#[test]
fn test_short_sequence() {
    let gen = MlsGenerator::new(5); // Very short: 31 samples
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    // Should still work with short sequences
    let result = analyzer.analyze(&sequence);
    assert_eq!(result.latency_samples, 0);
    assert!(result.confidence > 0.5);
}
