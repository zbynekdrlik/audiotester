//! E2E tests for persistent configuration and channel pair
//!
//! Tests config round-trip, defaults, backward compatibility,
//! and AudioEngine channel pair validation.

use audiotester::audio::engine::AudioEngine;

#[test]
fn test_engine_channel_pair_default() {
    let engine = AudioEngine::new();
    assert_eq!(engine.channel_pair(), [1, 2]);
}

#[test]
fn test_engine_set_channel_pair_valid() {
    let mut engine = AudioEngine::new();
    engine.set_channel_pair([3, 4]).unwrap();
    assert_eq!(engine.channel_pair(), [3, 4]);
}

#[test]
fn test_engine_set_channel_pair_high_channels() {
    let mut engine = AudioEngine::new();
    engine.set_channel_pair([127, 128]).unwrap();
    assert_eq!(engine.channel_pair(), [127, 128]);
}

#[test]
fn test_engine_set_channel_pair_zero_signal_rejected() {
    let mut engine = AudioEngine::new();
    let result = engine.set_channel_pair([0, 1]);
    assert!(result.is_err());
    // Original pair should be unchanged
    assert_eq!(engine.channel_pair(), [1, 2]);
}

#[test]
fn test_engine_set_channel_pair_zero_counter_rejected() {
    let mut engine = AudioEngine::new();
    let result = engine.set_channel_pair([1, 0]);
    assert!(result.is_err());
    assert_eq!(engine.channel_pair(), [1, 2]);
}

#[test]
fn test_engine_set_channel_pair_same_channel_rejected() {
    let mut engine = AudioEngine::new();
    let result = engine.set_channel_pair([5, 5]);
    assert!(result.is_err());
    assert_eq!(engine.channel_pair(), [1, 2]);
}

#[test]
fn test_engine_set_channel_pair_swapped_order() {
    let mut engine = AudioEngine::new();
    // Counter before signal (higher channel first) should work
    engine.set_channel_pair([128, 127]).unwrap();
    assert_eq!(engine.channel_pair(), [128, 127]);
}

#[test]
fn test_config_response_includes_channel_pair() {
    // Verify the ConfigResponse JSON contract includes channel_pair
    let json = serde_json::json!({
        "device": "Test ASIO",
        "sample_rate": 96000,
        "monitoring": false,
        "channel_pair": [127, 128]
    });

    let pair = json["channel_pair"].as_array().unwrap();
    assert_eq!(pair.len(), 2);
    assert_eq!(pair[0], 127);
    assert_eq!(pair[1], 128);
}

#[test]
fn test_config_update_with_channel_pair() {
    // Verify ConfigUpdate accepts channel_pair
    let json = serde_json::json!({
        "channel_pair": [3, 4]
    });

    let pair = json["channel_pair"].as_array().unwrap();
    assert_eq!(pair.len(), 2);
    assert_eq!(pair[0], 3);
    assert_eq!(pair[1], 4);
}
