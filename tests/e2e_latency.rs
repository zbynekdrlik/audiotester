//! E2E tests for latency measurement
//!
//! Tests both the new burst-based latency detection system and
//! legacy MLS cross-correlation for backward compatibility.

use audiotester::audio::burst::BurstGenerator;
use audiotester::audio::detector::BurstDetector;
use audiotester::audio::latency::LatencyAnalyzer;
use audiotester::audio::{analyzer::Analyzer, signal::MlsGenerator};
use std::time::{Duration, Instant};

// ============================================================================
// BURST-BASED LATENCY TESTS (Primary System)
// ============================================================================

/// Test burst generator produces correct signal structure
#[test]
fn test_burst_generator_structure() {
    let mut gen = BurstGenerator::new(48000);
    let cycle_len = gen.cycle_length();

    assert_eq!(cycle_len, 4800, "100ms at 48kHz should be 4800 samples");

    // First 90% should be silence
    for i in 0..gen.burst_start_position() {
        let (sample, is_start) = gen.next_sample();
        assert_eq!(sample, 0.0, "Sample {} should be silence", i);
        assert!(!is_start, "Sample {} should not be burst start", i);
    }

    // Burst start should be flagged
    let (_sample, is_start) = gen.next_sample();
    assert!(is_start, "First burst sample should be marked as start");
    // Note: First burst sample value is random noise, may be any value including zero
}

/// Test burst detector finds burst onset
#[test]
fn test_burst_detection() {
    let mut detector = BurstDetector::new(48000);

    // Process silence to establish noise floor
    for i in 0..1000 {
        let result = detector.process(0.0, i);
        assert!(result.is_none(), "Silence should not trigger detection");
    }

    // Process burst signal
    let mut detected = false;
    for i in 0..100 {
        if detector.process(0.5, 1000 + i).is_some() {
            detected = true;
            break;
        }
    }

    assert!(detected, "Burst should be detected within 100 samples");
}

/// Test latency calculation with simulated delay
#[test]
fn test_burst_latency_calculation() {
    let mut gen = BurstGenerator::new(48000);
    let mut analyzer = LatencyAnalyzer::new(48000);

    // Generate one cycle of burst signal
    let mut output_buffer = vec![0.0f32; gen.cycle_length()];
    let burst_starts = gen.fill_buffer(&mut output_buffer);

    assert_eq!(
        burst_starts.len(),
        1,
        "Should have exactly one burst per cycle"
    );

    // Register burst event at start of burst
    let burst_time = Instant::now();
    let event = audiotester::audio::burst::BurstEvent {
        start_time: burst_time,
        start_frame: burst_starts[0] as u64,
    };
    analyzer.register_burst(event);

    // Simulate 5ms delay
    std::thread::sleep(Duration::from_millis(5));

    // Create input buffer (just the burst portion, starting at detection point)
    let burst_start = gen.burst_start_position();
    let input_buffer = output_buffer[burst_start..].to_vec();

    let callback_time = Instant::now();
    let result = analyzer.analyze(&input_buffer, callback_time);

    assert!(
        result.is_some(),
        "Should detect burst and calculate latency"
    );
    let result = result.unwrap();

    // Latency should be approximately 5ms (with some jitter tolerance)
    assert!(
        result.latency_ms > 2.0 && result.latency_ms < 20.0,
        "Latency should be approximately 5ms, got {}ms",
        result.latency_ms
    );
}

/// Test burst detection update rate
#[test]
fn test_burst_update_rate() {
    let gen = BurstGenerator::new(96000);
    assert!(
        (gen.update_rate() - 10.0).abs() < 0.1,
        "Should have 10Hz update rate"
    );
}

/// Test detector handles varying noise floors
#[test]
fn test_burst_noise_tolerance() {
    let mut detector = BurstDetector::new(48000);

    // Process low-level noise
    let mut seed = 12345u32;
    for i in 0..2000 {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let noise = ((seed >> 16) as f32 / 32768.0 - 1.0) * 0.01; // Low noise
        detector.process(noise, i);
    }

    // Burst should still be detectable above noise
    let mut detected = false;
    for i in 0..50 {
        if detector.process(0.5, 2000 + i).is_some() {
            detected = true;
            break;
        }
    }

    assert!(detected, "Burst should be detectable above noise floor");
}

/// Test multiple burst cycles
#[test]
fn test_continuous_burst_monitoring() {
    let mut gen = BurstGenerator::new(48000);
    let cycle_len = gen.cycle_length();

    let mut burst_count = 0;
    for cycle in 0..5 {
        for _ in 0..cycle_len {
            let (_, is_start) = gen.next_sample();
            if is_start {
                burst_count += 1;
            }
        }
        assert_eq!(burst_count, cycle + 1, "Should have one burst per cycle");
    }
}

// ============================================================================
// LEGACY MLS CORRELATION TESTS (Backward Compatibility)
// ============================================================================

/// Test latency detection with no delay (legacy MLS)
#[test]
fn test_zero_latency_mls() {
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

/// Test latency detection with various delays (legacy MLS)
#[test]
fn test_known_delays_mls() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();

    let test_delays = [1, 10, 100, 480, 1000, 2000];

    for &delay in &test_delays {
        let mut analyzer = Analyzer::new(&sequence, 48000);

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

/// Test latency in milliseconds calculation (legacy MLS)
#[test]
fn test_latency_ms_calculation_mls() {
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

/// Test different sample rates (legacy MLS)
#[test]
fn test_sample_rate_independence_mls() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();

    let sample_rates = [44100, 48000, 96000];
    let target_ms = 10.0;

    for &sr in &sample_rates {
        let mut analyzer = Analyzer::new(&sequence, sr);

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

/// Test detection with noise (legacy MLS)
#[test]
fn test_noise_tolerance_mls() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 100;

    let mut noisy: Vec<f32> = vec![0.0f32; delay];
    noisy.extend(&sequence);

    let noise_level = 0.1;
    let mut seed: u32 = 12345;
    for sample in &mut noisy {
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

/// Test detection with amplitude variations (legacy MLS)
#[test]
fn test_amplitude_invariance_mls() {
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

/// Test with inverted signal (legacy MLS)
#[test]
fn test_inverted_signal_mls() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 150;
    let mut inverted: Vec<f32> = vec![0.0f32; delay];
    inverted.extend(sequence.iter().map(|&x| -x));

    let result = analyzer.analyze(&inverted);

    assert_eq!(
        result.latency_samples, delay,
        "Inverted signal should still be detected"
    );
}

/// Test insufficient buffer handling (legacy MLS)
#[test]
fn test_insufficient_buffer_mls() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let short_buffer = [0.0f32; 100];
    let result = analyzer.analyze(&short_buffer);

    assert!(
        !result.is_healthy,
        "Insufficient buffer should report unhealthy"
    );
}

/// Test reset functionality (legacy MLS)
#[test]
fn test_analyzer_reset_mls() {
    let gen = MlsGenerator::new(12);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let mut delayed = vec![0.0f32; 100];
    delayed.extend(&sequence);
    let result1 = analyzer.analyze(&delayed);

    analyzer.reset();

    let result2 = analyzer.analyze(&delayed);

    assert_eq!(
        result1.latency_samples, result2.latency_samples,
        "Reset analyzer should produce consistent results"
    );
}

/// Test continuous monitoring (legacy MLS)
#[test]
fn test_continuous_monitoring_mls() {
    let gen = MlsGenerator::new(10);
    let sequence = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&sequence, 48000);

    let delay = 50;
    let mut delayed = vec![0.0f32; delay];
    delayed.extend(&sequence);

    for iteration in 0..10 {
        let result = analyzer.analyze(&delayed);
        assert_eq!(
            result.latency_samples, delay,
            "Iteration {}: latency should be consistent",
            iteration
        );
    }
}
