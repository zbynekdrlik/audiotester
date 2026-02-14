//! E2E tests for latency measurement
//!
//! Tests both the new frame-based burst latency detection system and
//! legacy MLS cross-correlation for backward compatibility.
//!
//! ## Frame-Based Latency Measurement
//!
//! The new system uses sample frame counters instead of wall-clock timestamps:
//! - Output callback generates burst at output_frame N
//! - Input callback detects burst at input_frame M
//! - Latency = (M - N) / sample_rate
//!
//! This eliminates the ~500ms artificial delay from ring buffer accumulation.

use audiotester::audio::burst::{BurstEvent, BurstGenerator, DetectionEvent};
use audiotester::audio::detector::BurstDetector;
use audiotester::audio::latency::LatencyAnalyzer;
use audiotester::audio::{analyzer::Analyzer, signal::MlsGenerator};
use std::time::Duration;

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

/// Test frame-based latency calculation
#[test]
fn test_burst_latency_calculation() {
    let gen = BurstGenerator::new(48000);
    let mut analyzer = LatencyAnalyzer::new(48000);

    // Burst starts at frame 1000
    let output_frame = 1000u64;
    let event = BurstEvent {
        start_frame: output_frame,
    };
    analyzer.register_burst(event);

    // Simulate 5ms latency: 5ms * 48000 = 240 samples
    let input_frame = output_frame + 240;
    let detection = DetectionEvent { input_frame };

    let result = analyzer.match_detection(&detection);

    assert!(result.is_some(), "Should match burst and calculate latency");
    let result = result.unwrap();

    // Latency should be exactly 5ms (frame-based is precise)
    assert_eq!(result.latency_samples, 240);
    assert!(
        (result.latency_ms - 5.0).abs() < 0.1,
        "Latency should be 5ms, got {}ms",
        result.latency_ms
    );

    // Test burst generator cycle length for reference
    assert_eq!(
        gen.cycle_length(),
        4800,
        "100ms at 48kHz should be 4800 samples"
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

// ============================================================================
// HARDWARE E2E TESTS (Real ASIO Hardware on iem.lan)
// ============================================================================
//
// These tests verify that the latency measurement system works correctly
// with real ASIO hardware. They call the actual deployed API on iem.lan.
//
// IMPORTANT: These tests require:
// - audiotester deployed and running on iem.lan
// - VASIO-8 loopback configured
// - Network access to http://iem.lan:8920
//
// Run with: cargo test --test e2e_latency -- --ignored --test-threads=1

/// Test latency on iem.lan matches expected hardware performance
///
/// This test calls the actual deployed service on real ASIO hardware.
/// Ableton shows ~4ms on the same VASIO-8 loopback path, so we expect
/// our measurement to be under 10ms (allowing for measurement overhead).
#[test]
#[ignore = "requires iem.lan deployment"]
fn test_iem_lan_latency_under_10ms() {
    // Call the actual deployed API
    let response = reqwest::blocking::get("http://iem.lan:8920/api/v1/stats")
        .expect("iem.lan not reachable - is audiotester deployed?");

    assert!(
        response.status().is_success(),
        "API returned error: {}",
        response.status()
    );

    let stats: serde_json::Value = response.json().expect("Invalid JSON response");

    let latency_ms = stats["current_latency_ms"]
        .as_f64()
        .expect("Missing current_latency_ms in response");

    // CRITICAL: This is the acceptance criteria
    // Ableton shows ~4ms, so we should be under 10ms
    assert!(
        latency_ms < 10.0,
        "Latency {}ms exceeds 10ms threshold. \
         Ableton shows ~4ms on same VASIO-8 loopback. \
         The measurement implementation may be broken.",
        latency_ms
    );

    // Also verify latency is positive and reasonable
    assert!(
        latency_ms > 0.1,
        "Latency {}ms is suspiciously low - measurement may not be working",
        latency_ms
    );

    println!("✓ Measured latency: {:.2}ms (threshold: <10ms)", latency_ms);
}

/// Test that latency is stable (not cycling) on iem.lan
///
/// Takes 10 measurements over 2 seconds and verifies the standard deviation
/// is under 2ms. High variance indicates cycling or jitter issues.
#[test]
#[ignore = "requires iem.lan deployment"]
fn test_iem_lan_latency_stable() {
    let mut measurements = Vec::new();

    // Take 10 measurements over 2 seconds
    for i in 0..10 {
        let response = reqwest::blocking::get("http://iem.lan:8920/api/v1/stats")
            .unwrap_or_else(|_| panic!("iem.lan not reachable on measurement {}", i));

        let stats: serde_json::Value = response.json().unwrap();
        if let Some(latency) = stats["current_latency_ms"].as_f64() {
            measurements.push(latency);
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    assert!(
        measurements.len() >= 5,
        "Not enough measurements collected: {}",
        measurements.len()
    );

    // Calculate variance
    let mean = measurements.iter().sum::<f64>() / measurements.len() as f64;
    let variance =
        measurements.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / measurements.len() as f64;
    let std_dev = variance.sqrt();

    // Latency should be stable within 2ms
    assert!(
        std_dev < 2.0,
        "Latency unstable: std_dev {:.2}ms, measurements: {:?}. \
         This indicates cycling or jitter issues.",
        std_dev,
        measurements
    );

    println!(
        "✓ Latency stable: mean={:.2}ms, std_dev={:.2}ms",
        mean, std_dev
    );
}

/// Test no false sample loss on healthy loopback
///
/// On a healthy VASIO-8 loopback, there should be zero sample loss.
/// False positives indicate bugs in the measurement system.
#[test]
#[ignore = "requires iem.lan deployment"]
fn test_iem_lan_no_false_loss() {
    let response =
        reqwest::blocking::get("http://iem.lan:8920/api/v1/stats").expect("iem.lan not reachable");

    let stats: serde_json::Value = response.json().expect("Invalid JSON response");

    let lost = stats["total_lost"].as_u64().unwrap_or(0);

    assert_eq!(
        lost, 0,
        "Detected {} lost samples on healthy loopback. \
         This is likely a false positive from measurement bugs.",
        lost
    );

    println!("✓ No sample loss detected (expected: 0)");
}

// ============================================================================
// FRAME-BASED LATENCY UNIT TESTS
// ============================================================================

/// Test frame-based latency at various sample rates
#[test]
fn test_frame_latency_various_sample_rates() {
    let sample_rates = [44100u32, 48000, 96000, 192000];
    let target_latency_ms = 4.0; // Target 4ms like Ableton

    for &sr in &sample_rates {
        let mut analyzer = LatencyAnalyzer::new(sr);

        // Register burst at frame 10000
        let output_frame = 10000u64;
        analyzer.register_burst(BurstEvent {
            start_frame: output_frame,
        });

        // Calculate input frame for target latency
        let latency_samples = ((target_latency_ms / 1000.0) * sr as f64).round() as u64;
        let input_frame = output_frame + latency_samples;

        let result = analyzer
            .match_detection(&DetectionEvent { input_frame })
            .expect("Should match");

        assert!(
            (result.latency_ms - target_latency_ms).abs() < 0.5,
            "At {}Hz, {}ms latency should be detected (got {:.2}ms)",
            sr,
            target_latency_ms,
            result.latency_ms
        );
    }
}

/// Test that frame-based measurement handles multiple bursts correctly
#[test]
fn test_frame_latency_multiple_bursts() {
    let mut analyzer = LatencyAnalyzer::new(48000);

    // Register multiple bursts
    for i in 0..5 {
        let output_frame = i * 4800; // One burst every 100ms
        analyzer.register_burst(BurstEvent {
            start_frame: output_frame,
        });
    }

    // Match detections with consistent 3ms latency (144 samples at 48kHz)
    for i in 0..5 {
        let output_frame = i * 4800;
        let input_frame = output_frame + 144;

        let result = analyzer
            .match_detection(&DetectionEvent { input_frame })
            .expect("Should match burst");

        assert_eq!(
            result.latency_samples, 144,
            "Burst {} should have 144 sample latency",
            i
        );
        assert!(
            (result.latency_ms - 3.0).abs() < 0.1,
            "Burst {} should have ~3ms latency, got {:.2}ms",
            i,
            result.latency_ms
        );
    }
}
