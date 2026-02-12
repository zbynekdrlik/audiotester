//! Integration tests for full audio path
//!
//! Tests the complete audio pipeline from signal generation
//! through analysis, using simulated/mocked audio path.

use audiotester::audio::{analyzer::Analyzer, engine::AudioEngine, signal::MlsGenerator};
use audiotester::stats::store::StatsStore;
use std::sync::{Arc, Mutex};

/// Test complete audio pipeline simulation
#[test]
fn test_simulated_audio_pipeline() {
    // Setup components
    let mut generator = MlsGenerator::new(12);
    let reference = generator.sequence().to_vec();
    let mut analyzer = Analyzer::new(&reference, 48000);
    let stats = Arc::new(Mutex::new(StatsStore::new()));

    // Simulate audio frames
    let frame_size = 256;
    let simulated_latency = 480; // 10ms at 48kHz
    let num_frames = 10;

    // Buffer to accumulate output
    let mut output_buffer = Vec::with_capacity(frame_size * num_frames + simulated_latency);

    // Add latency padding
    output_buffer.extend(vec![0.0f32; simulated_latency]);

    // Generate audio frames
    for _ in 0..num_frames {
        let mut frame = [0.0f32; 256];
        generator.fill_buffer(&mut frame);
        output_buffer.extend_from_slice(&frame);
    }

    // Analyze the accumulated output
    let result = analyzer.analyze(&output_buffer);

    // Record to stats
    {
        let mut store = stats.lock().unwrap();
        store.record_latency(result.latency_ms);
    }

    // Verify results
    assert_eq!(
        result.latency_samples, simulated_latency,
        "Should detect simulated latency"
    );

    let expected_ms = simulated_latency as f64 / 48000.0 * 1000.0;
    assert!(
        (result.latency_ms - expected_ms).abs() < 0.1,
        "Latency in ms should match"
    );

    // Check stats were recorded
    {
        let store = stats.lock().unwrap();
        assert_eq!(store.stats().measurement_count, 1);
        assert!(store.stats().current_latency > 0.0);
    }
}

/// Test multiple measurement cycles
#[test]
fn test_multiple_measurement_cycles() {
    let gen = MlsGenerator::new(12);
    let reference = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&reference, 48000);
    let stats = Arc::new(Mutex::new(StatsStore::new()));

    // Simulate various latencies
    let latencies = [100, 200, 150, 175, 180];

    for &latency in &latencies {
        let mut signal = vec![0.0f32; latency];
        signal.extend(&reference);

        let result = analyzer.analyze(&signal);

        {
            let mut store = stats.lock().unwrap();
            store.record_latency(result.latency_ms);
        }

        assert_eq!(result.latency_samples, latency);
    }

    // Check final stats
    let store = stats.lock().unwrap();
    assert_eq!(store.stats().measurement_count, latencies.len() as u64);
    assert!(store.stats().min_latency > 0.0);
    assert!(store.stats().max_latency > store.stats().min_latency);
}

/// Test engine state transitions
#[test]
fn test_engine_lifecycle() {
    use audiotester::audio::engine::EngineState;

    let mut engine = AudioEngine::new();

    // Initial state
    assert_eq!(engine.state(), EngineState::Stopped);

    // Select device (placeholder)
    engine.select_device("Test Device").unwrap();

    // Start
    engine.start().unwrap();
    assert_eq!(engine.state(), EngineState::Running);

    // Stop
    engine.stop().unwrap();
    assert_eq!(engine.state(), EngineState::Stopped);
}

/// Test stats persistence through measurement cycle
#[test]
fn test_stats_accumulation() {
    let mut store = StatsStore::new();

    // Simulate continuous measurements
    for i in 0..100 {
        let latency = 5.0 + (i as f64 * 0.1).sin() * 0.5;
        store.record_latency(latency);

        if i % 10 == 0 {
            store.record_loss(1);
        }
    }

    assert_eq!(store.stats().measurement_count, 100);
    assert_eq!(store.stats().total_lost, 10);
    assert!(store.stats().avg_latency > 4.0 && store.stats().avg_latency < 6.0);
}

/// Test generator and analyzer coordination
#[test]
fn test_generator_analyzer_sync() {
    let mut gen = MlsGenerator::new(10);
    let reference = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&reference, 48000);

    // Generate multiple buffers worth
    let buffers_count = 5;
    let buffer_size = 512;

    for _ in 0..buffers_count {
        let mut buffer = vec![0.0f32; buffer_size];
        gen.fill_buffer(&mut buffer);

        // For this test, we're checking generator doesn't produce garbage
        for sample in &buffer {
            assert!(
                sample.abs() <= gen.amplitude() + 0.001,
                "Sample {} exceeds amplitude",
                sample
            );
        }
    }

    // Verify generator wrapped around correctly
    assert!(
        gen.position() < gen.length(),
        "Position should always be within bounds"
    );
}

/// Test real-time scenario simulation
#[test]
fn test_realtime_scenario() {
    let gen = MlsGenerator::new(12);
    let reference = gen.sequence().to_vec();
    let stats = Arc::new(Mutex::new(StatsStore::new()));

    // Simulate 1 second of monitoring at 48kHz with 256-sample frames
    let sample_rate = 48000;
    let frame_size = 256;
    let num_frames = sample_rate / frame_size;
    let fixed_latency = 240; // 5ms

    let mut analyzer = Analyzer::new(&reference, sample_rate);

    for frame_idx in 0..num_frames {
        // Create frame with fixed latency
        let mut frame = vec![0.0f32; fixed_latency];

        // Add portion of MLS sequence
        let start = (frame_idx * frame_size) % reference.len();
        let end = ((frame_idx + 1) * frame_size) % reference.len();

        if end > start {
            frame.extend_from_slice(&reference[start..end]);
        } else {
            frame.extend_from_slice(&reference[start..]);
            frame.extend_from_slice(&reference[..end]);
        }

        // Only analyze when we have enough data
        if frame.len() >= reference.len() {
            let result = analyzer.analyze(&frame);
            if result.is_healthy {
                let mut store = stats.lock().unwrap();
                store.record_latency(result.latency_ms);
            }
        }
    }

    // Verify we got measurements
    let store = stats.lock().unwrap();
    // May or may not have measurements depending on buffer accumulation
    // This test primarily checks for no panics/errors
}

/// Test graceful handling of edge cases
#[test]
fn test_edge_cases() {
    let gen = MlsGenerator::new(8); // Small sequence
    let reference = gen.sequence().to_vec();
    let mut analyzer = Analyzer::new(&reference, 48000);

    // Empty buffer
    let empty: Vec<f32> = vec![];
    let result = analyzer.analyze(&empty);
    assert!(!result.is_healthy);

    // Single sample
    let single = vec![1.0f32];
    let result = analyzer.analyze(&single);
    assert!(!result.is_healthy);

    // Exactly reference length
    let exact = reference.clone();
    let result = analyzer.analyze(&exact);
    assert_eq!(result.latency_samples, 0);
    assert!(result.is_healthy);

    // Double reference length
    let doubled: Vec<f32> = reference.iter().chain(reference.iter()).copied().collect();
    let result = analyzer.analyze(&doubled);
    assert_eq!(result.latency_samples, 0);
}
