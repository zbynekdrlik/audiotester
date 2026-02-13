//! Meta-tests that verify test suite integrity
//!
//! These tests ensure that:
//! - No tests are ignored
//! - E2E test files exist
//! - Latency measurements are bounded (no aliasing artifacts)

use std::process::Command;

/// Verify no tests are ignored in the workspace
///
/// Ignored tests can hide regressions. All tests must run.
#[test]
fn no_ignored_tests() {
    let output = Command::new("cargo")
        .args(["test", "--workspace", "--", "--list", "--ignored"])
        .output()
        .expect("Failed to run cargo test --list --ignored");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Count lines that indicate test functions
    let ignored_count = stdout.lines().filter(|l| l.contains(": test")).count();

    assert_eq!(
        ignored_count, 0,
        "Found {} ignored tests - all tests must run.\n\
         Ignored tests:\n{}",
        ignored_count, stdout
    );
}

/// Verify E2E test files exist and are not empty
#[test]
fn e2e_tests_exist() {
    let test_files = ["e2e_signal.rs", "e2e_latency.rs", "e2e_loss.rs"];

    for file in test_files {
        let path = format!("tests/{}", file);
        let full_path = std::path::Path::new(&path);

        assert!(
            full_path.exists(),
            "Missing E2E test file: {}. All E2E tests must be present.",
            file
        );

        // Check file is not empty
        let metadata = std::fs::metadata(full_path).expect("Failed to get file metadata");
        assert!(
            metadata.len() > 100,
            "E2E test file {} appears to be empty or too small ({} bytes)",
            file,
            metadata.len()
        );
    }
}

/// Verify latency measurement is bounded (no cycling 0-680ms)
///
/// This test creates an analyzer and verifies that returned latency
/// is always within expected bounds (0-100ms), preventing the
/// FFT circular correlation aliasing issue.
#[test]
fn latency_measurement_is_bounded() {
    use audiotester::audio::{analyzer::Analyzer, signal::MlsGenerator};

    let gen = MlsGenerator::new(15); // Order 15 = 32767 samples (production config)
    let sequence = gen.sequence().to_vec();

    // Test at 96kHz (production sample rate)
    let sample_rate = 96000u32;
    let mut analyzer = Analyzer::new(&sequence, sample_rate);

    // Maximum expected latency in samples (100ms at 96kHz)
    let max_latency_samples = (sample_rate / 10) as usize;

    // Test various delays to ensure they're detected within bounds
    let test_delays = [0, 100, 500, 1000, 5000, 9000];

    for &delay in &test_delays {
        // Create signal with known delay
        let mut delayed: Vec<f32> = vec![0.0f32; delay];
        delayed.extend(&sequence);

        let result = analyzer.analyze(&delayed);

        assert!(
            result.latency_samples <= max_latency_samples,
            "Latency {} samples exceeds maximum {} samples (100ms) - \
             correlation aliasing may be occurring. \
             Test delay was {} samples.",
            result.latency_samples,
            max_latency_samples,
            delay
        );

        // If delay is within bounds, it should be detected accurately
        if delay <= max_latency_samples {
            assert_eq!(
                result.latency_samples, delay,
                "Delay of {} samples should be detected correctly, got {}",
                delay, result.latency_samples
            );
        }

        analyzer.reset();
    }
}

/// Verify that the latency never cycles through MLS period multiples
///
/// This catches the specific bug where latency would jump between
/// 0, ~341ms, ~682ms due to FFT circular correlation aliasing.
#[test]
fn no_latency_cycling() {
    use audiotester::audio::{analyzer::Analyzer, signal::MlsGenerator};

    let gen = MlsGenerator::new(15);
    let sequence = gen.sequence().to_vec();
    let mls_period = sequence.len(); // 32767 samples

    let sample_rate = 96000u32;
    let mut analyzer = Analyzer::new(&sequence, sample_rate);

    // Simulate the problematic scenario: small actual latency
    let actual_delay = 128; // 1.33ms at 96kHz
    let mut delayed: Vec<f32> = vec![0.0f32; actual_delay];
    delayed.extend(&sequence);

    let result = analyzer.analyze(&delayed);

    // Latency should NEVER be near MLS period multiples
    let forbidden_ranges = [
        (mls_period - 1000, mls_period + 1000),         // ~341ms
        (2 * mls_period - 1000, 2 * mls_period + 1000), // ~682ms
    ];

    for (low, high) in forbidden_ranges {
        assert!(
            result.latency_samples < low || result.latency_samples > high,
            "Latency {} samples is suspiciously close to MLS period multiple {}. \
             This indicates FFT circular correlation aliasing is not being constrained.",
            result.latency_samples,
            if low < mls_period + 1000 {
                mls_period
            } else {
                2 * mls_period
            }
        );
    }

    // Verify actual latency is detected
    assert_eq!(
        result.latency_samples, actual_delay,
        "Expected latency of {} samples, got {}",
        actual_delay, result.latency_samples
    );
}

/// Verify frame loss detection doesn't produce false positives from latency cycling
#[test]
fn no_false_loss_from_latency_cycling() {
    use audiotester::audio::{analyzer::Analyzer, signal::MlsGenerator};

    let gen = MlsGenerator::new(15);
    let sequence = gen.sequence().to_vec();

    let sample_rate = 96000u32;
    let mut analyzer = Analyzer::new(&sequence, sample_rate);

    // Create perfect signal with small delay (no actual loss)
    let delay = 128;
    let mut delayed: Vec<f32> = vec![0.0f32; delay];
    delayed.extend(&sequence);

    // Run multiple analysis cycles to simulate continuous monitoring
    for iteration in 0..10 {
        let result = analyzer.analyze(&delayed);

        // Should never report massive sample loss from aliasing artifacts
        // The old bug would report ~64,536 lost samples per cycle
        assert!(
            result.lost_samples < 1000,
            "Iteration {}: Reported {} lost samples which is suspiciously high. \
             This may indicate false positives from latency measurement cycling.",
            iteration,
            result.lost_samples
        );
    }
}
