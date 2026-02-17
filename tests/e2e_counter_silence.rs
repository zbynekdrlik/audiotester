//! E2E tests for counter channel silence detection
//!
//! Verifies that ch1 counter silence (muted loopback) is detected correctly,
//! recovery produces zero false loss, and counter wrap-around doesn't trigger
//! false silence.

use audiotester_core::audio::analyzer::{Analyzer, FrameLossResult};

/// Test that feeding all-zero counter samples triggers silence detection
/// after the threshold (sample_rate / 10 = 100ms worth of zeros).
#[test]
fn test_counter_silence_detection_48khz() {
    let sample_rate = 48000;
    let mut analyzer = Analyzer::new(&[], sample_rate);

    // First, establish a baseline with normal counter values
    let baseline: Vec<f32> = (0..100).map(|i| i as f32 / 65536.0).collect();
    let result = analyzer.detect_frame_loss(&baseline);
    assert!(
        !result.counter_silent,
        "Should not be silent with normal signal"
    );

    // Now feed silence — need threshold = 48000/10 = 4800 consecutive zeros
    let silence = vec![0.0f32; 5000];
    let result = analyzer.detect_frame_loss(&silence);
    assert!(
        result.counter_silent,
        "Should detect counter silence after {} zero samples at {}Hz",
        5000, sample_rate
    );
    assert_eq!(
        result.confirmed_lost, 0,
        "Silence should not report confirmed lost samples"
    );
}

/// Test silence detection at 96kHz (threshold = 9600 samples)
#[test]
fn test_counter_silence_detection_96khz() {
    let sample_rate = 96000;
    let mut analyzer = Analyzer::new(&[], sample_rate);

    // Establish baseline
    let baseline: Vec<f32> = (0..100).map(|i| i as f32 / 65536.0).collect();
    let _ = analyzer.detect_frame_loss(&baseline);

    // Feed silence — need threshold = 96000/10 = 9600 consecutive zeros
    // Feed slightly more than threshold
    let silence = vec![0.0f32; 10000];
    let result = analyzer.detect_frame_loss(&silence);
    assert!(
        result.counter_silent,
        "Should detect counter silence at 96kHz"
    );
}

/// Test that silence detection does NOT trigger below the threshold.
#[test]
fn test_counter_silence_below_threshold() {
    let sample_rate = 48000;
    let mut analyzer = Analyzer::new(&[], sample_rate);

    // Establish baseline
    let baseline: Vec<f32> = (0..100).map(|i| i as f32 / 65536.0).collect();
    let _ = analyzer.detect_frame_loss(&baseline);

    // Feed fewer zeros than the threshold (4800)
    let short_silence = vec![0.0f32; 2000];
    let result = analyzer.detect_frame_loss(&short_silence);
    assert!(
        !result.counter_silent,
        "Should NOT detect silence below threshold"
    );
}

/// Test that recovery from silence produces zero confirmed loss.
/// This is the critical fix: on unmute, the counter resumes at a much higher
/// value, but the analyzer should resync instead of reporting a massive gap.
#[test]
fn test_counter_recovery_no_false_loss() {
    let sample_rate = 48000;
    let mut analyzer = Analyzer::new(&[], sample_rate);

    // Phase 1: Normal operation (counter 0-99)
    let normal: Vec<f32> = (0..100).map(|i| i as f32 / 65536.0).collect();
    let result = analyzer.detect_frame_loss(&normal);
    assert_eq!(result.confirmed_lost, 0);
    assert!(!result.counter_silent);

    // Phase 2: Mute — feed enough zeros to trigger silence
    let silence = vec![0.0f32; 5000];
    let result = analyzer.detect_frame_loss(&silence);
    assert!(result.counter_silent, "Should be in silence state");

    // Phase 3: Unmute — counter resumes at a completely different value
    // (e.g., 5 seconds later at 96kHz = 480000 frames, counter wraps ~7 times)
    let resume_start = 14000u32; // Arbitrary counter value after mute
    let recovery: Vec<f32> = (0..200)
        .map(|i| ((resume_start + i) & 0xFFFF) as f32 / 65536.0)
        .collect();
    let result = analyzer.detect_frame_loss(&recovery);

    assert_eq!(
        result.confirmed_lost, 0,
        "Recovery from silence should produce zero confirmed loss (resynced), got {}",
        result.confirmed_lost
    );
    assert!(
        !result.counter_silent,
        "Should exit silence state after receiving non-zero values"
    );
}

/// Test that the counter naturally wrapping through zero doesn't trigger
/// false silence detection. The counter passes through zero once per 65536
/// frames — that's a single zero-valued sample, far below the threshold.
#[test]
fn test_counter_wrap_through_zero_not_silence() {
    let sample_rate = 48000;
    let mut analyzer = Analyzer::new(&[], sample_rate);

    // Counter values wrapping from 65530 through 0 to 10
    let mut samples = Vec::new();
    for i in 65530u32..65536 {
        samples.push(i as f32 / 65536.0);
    }
    // Counter value 0 encodes as 0.0 — this single zero should NOT trigger silence
    for i in 0u32..10 {
        samples.push(i as f32 / 65536.0);
    }

    let result = analyzer.detect_frame_loss(&samples);
    assert!(
        !result.counter_silent,
        "Natural counter wrap through zero should not trigger silence"
    );
    assert_eq!(
        result.confirmed_lost, 0,
        "Counter wrap should not produce loss"
    );
}

/// Test that real gaps are still detected during normal (non-silent) operation.
#[test]
fn test_real_gaps_still_detected() {
    let mut analyzer = Analyzer::new(&[], 48000);

    // Counter 0-99, then skip to 105-199 (missing 100-104 = 5 frames)
    let mut samples = Vec::new();
    for i in 0u32..100 {
        samples.push(i as f32 / 65536.0);
    }
    // Skip 5 frames
    for i in 105u32..200 {
        samples.push(i as f32 / 65536.0);
    }

    let result = analyzer.detect_frame_loss(&samples);
    assert!(
        result.confirmed_lost >= 4,
        "Should still detect real gaps: got {} lost",
        result.confirmed_lost
    );
    assert!(
        !result.counter_silent,
        "Active counter with gaps should not be silent"
    );
}

/// Test multiple silence/recovery cycles.
#[test]
fn test_multiple_silence_recovery_cycles() {
    let sample_rate = 48000;
    let mut analyzer = Analyzer::new(&[], sample_rate);

    for cycle in 0..3 {
        // Normal operation
        let start = cycle * 10000;
        let normal: Vec<f32> = (0..200)
            .map(|i| ((start + i) & 0xFFFF) as f32 / 65536.0)
            .collect();
        let result = analyzer.detect_frame_loss(&normal);
        assert!(
            !result.counter_silent,
            "Cycle {}: should not be silent during normal operation",
            cycle
        );

        // Go silent
        let silence = vec![0.0f32; 5000];
        let result = analyzer.detect_frame_loss(&silence);
        assert!(
            result.counter_silent,
            "Cycle {}: should detect silence",
            cycle
        );

        // Recovery at new counter position
        let resume = (cycle + 1) * 20000;
        let recovery: Vec<f32> = (0..200)
            .map(|i| ((resume + i) & 0xFFFF) as f32 / 65536.0)
            .collect();
        let result = analyzer.detect_frame_loss(&recovery);
        assert_eq!(
            result.confirmed_lost, 0,
            "Cycle {}: recovery should produce zero loss",
            cycle
        );
    }
}

/// Test that FrameLossResult has the correct default values.
#[test]
fn test_frame_loss_result_default() {
    let result = FrameLossResult::default();
    assert_eq!(result.confirmed_lost, 0);
    assert!(!result.counter_silent);
    assert_eq!(result.samples_analyzed, 0);
}
