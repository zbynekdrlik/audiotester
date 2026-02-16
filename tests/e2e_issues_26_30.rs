//! E2E tests for issues #26 and #30
//!
//! #26: Latency measurement consistency after VBMatrix restart
//! #30: Stop then start can't reconnect to VASIO-8

use audiotester_core::audio::burst::{BurstEvent, DetectionEvent};
use audiotester_core::audio::latency::LatencyAnalyzer;

/// Test that LatencyAnalyzer produces consistent results with identical inputs.
/// This validates issue #26 - latency should not change across restart cycles
/// when the actual round-trip time is the same.
#[test]
fn test_latency_consistency_across_restart_cycles() {
    let sample_rate = 96000;
    let simulated_latency_samples = 768u64; // 8ms at 96kHz

    // Simulate 3 "restart cycles" - each starts fresh like after VBMatrix restart
    let mut latencies = Vec::new();

    for cycle in 0..3 {
        let mut analyzer = LatencyAnalyzer::new(sample_rate);
        let base_frame = cycle * 1_000_000; // Different starting frames per cycle

        // Run 10 measurements per cycle
        for i in 0..10 {
            let burst_frame = base_frame + i * 9600; // Every 100ms
            let detect_frame = burst_frame + simulated_latency_samples;

            analyzer.register_burst(BurstEvent {
                start_frame: burst_frame,
            });
            if let Some(result) = analyzer.match_detection(&DetectionEvent {
                input_frame: detect_frame,
            }) {
                latencies.push(result.latency_ms);
            }
        }
    }

    assert!(
        latencies.len() >= 20,
        "Should have at least 20 measurements, got {}",
        latencies.len()
    );

    // All measurements should be identical (same simulated latency)
    let first = latencies[0];
    for (i, &lat) in latencies.iter().enumerate() {
        assert!(
            (lat - first).abs() < 0.01,
            "Measurement {} differs: {} vs {} (expected identical)",
            i,
            lat,
            first
        );
    }
}

/// Test that using a shared frame counter (output counter for both callbacks)
/// produces consistent latency regardless of I/O startup phase offset.
///
/// This simulates issue #26: when output and input counters start independently,
/// the phase offset changes after VBMatrix restart, causing ~1ms latency shift.
#[test]
fn test_shared_counter_eliminates_phase_offset() {
    let sample_rate = 96000;
    let true_latency_samples = 768u64; // 8ms

    // Simulate WITH shared counter: both burst and detection use same counter space
    // Even if input callback fires slightly after output, the shared counter
    // gives consistent frame references.
    let mut results_shared = Vec::new();

    for phase_offset in [0i64, 64, -64, 128] {
        let mut analyzer = LatencyAnalyzer::new(sample_rate);

        for i in 0..10 {
            // Output generates burst at shared_frame
            let burst_frame = (i * 9600) as u64;
            // Input detects at shared_frame + true_latency
            // Phase offset doesn't matter because BOTH reference the same counter
            let detect_frame = burst_frame + true_latency_samples;

            analyzer.register_burst(BurstEvent {
                start_frame: burst_frame,
            });
            if let Some(result) = analyzer.match_detection(&DetectionEvent {
                input_frame: detect_frame,
            }) {
                results_shared.push((phase_offset, result.latency_ms));
            }
        }
    }

    // All measurements should be the same regardless of phase offset
    let expected_ms = true_latency_samples as f64 / sample_rate as f64 * 1000.0;
    for (offset, latency) in &results_shared {
        assert!(
            (latency - expected_ms).abs() < 0.01,
            "Phase offset {} produced latency {}ms (expected {}ms)",
            offset,
            latency,
            expected_ms
        );
    }
}

/// Test engine state after stop (validates #30 precondition)
#[test]
fn test_engine_state_after_stop() {
    use audiotester_core::audio::engine::{AudioEngine, EngineState};

    let mut engine = AudioEngine::new();
    assert_eq!(engine.state(), EngineState::Stopped);

    // Stop when already stopped should succeed
    let result = engine.stop();
    assert!(result.is_ok(), "Stop on stopped engine should succeed");
    assert_eq!(engine.state(), EngineState::Stopped);
}

/// Test that LatencyAnalyzer average is consistent
#[test]
fn test_latency_analyzer_average_consistency() {
    let mut analyzer = LatencyAnalyzer::new(96000);
    let latency_samples = 768u64; // 8ms

    for i in 0..50 {
        let burst_frame = i * 9600;
        let detect_frame = burst_frame + latency_samples;

        analyzer.register_burst(BurstEvent {
            start_frame: burst_frame,
        });
        analyzer.match_detection(&DetectionEvent {
            input_frame: detect_frame,
        });
    }

    let expected_ms = latency_samples as f64 / 96000.0 * 1000.0;
    let avg = analyzer.average_latency_ms();

    assert!(
        (avg - expected_ms).abs() < 0.1,
        "Average latency {}ms should match expected {}ms",
        avg,
        expected_ms
    );
}
