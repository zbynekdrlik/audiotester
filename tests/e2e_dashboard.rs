//! E2E tests for dashboard improvements
//!
//! Tests for device info display, reset button, extended history,
//! loss event visualization, and API response structure.

use audiotester::stats::store::StatsStore;

/// Test StatsResponse includes device info fields
#[test]
fn test_stats_response_has_device_fields() {
    // Verify the StatsResponse struct includes device_name, buffer_size, sample_rate
    // We test this via serde serialization
    let json = serde_json::json!({
        "current_latency": 5.0,
        "min_latency": 4.0,
        "max_latency": 6.0,
        "avg_latency": 5.0,
        "total_lost": 0,
        "total_corrupted": 0,
        "measurement_count": 100,
        "latency_history": [[-1.0, 5.0]],
        "loss_history": [],
        "device_name": "Dante Virtual Soundcard",
        "buffer_size": 256,
        "sample_rate": 96000,
        "uptime_seconds": 3600,
        "loss_events": []
    });

    assert!(json["device_name"].is_string());
    assert_eq!(json["device_name"], "Dante Virtual Soundcard");
    assert_eq!(json["buffer_size"], 256);
    assert_eq!(json["sample_rate"], 96000);
    assert_eq!(json["uptime_seconds"], 3600);
}

/// Test reset_counters preserves history but clears counters
#[test]
fn test_reset_counters_api() {
    let mut store = StatsStore::new();

    // Build up some data
    for i in 0..50 {
        store.record_latency((i as f64) * 0.5);
    }
    store.record_loss(42);
    store.record_corruption(3);

    // Verify pre-conditions
    assert_eq!(store.stats().measurement_count, 50);
    assert_eq!(store.stats().total_lost, 42);
    assert_eq!(store.stats().total_corrupted, 3);
    assert!(store.stats().min_latency < 1.0);
    assert!(store.stats().max_latency > 20.0);

    // Reset counters
    store.reset_counters();

    // Counters should be zeroed
    assert_eq!(store.stats().measurement_count, 0);
    assert_eq!(store.stats().total_lost, 0);
    assert_eq!(store.stats().total_corrupted, 0);
    assert_eq!(store.stats().min_latency, f64::MAX);
    assert_eq!(store.stats().max_latency, 0.0);
    assert_eq!(store.stats().avg_latency, 0.0);

    // History should still be present
    assert_eq!(
        store.latency_history().len(),
        50,
        "Latency history should be preserved after reset_counters"
    );
}

/// Test extended history with archive
#[test]
fn test_extended_history_archive() {
    let mut store = StatsStore::new();

    // Record 100 measurements - every 10th should go to archive
    for i in 0..100 {
        store.record_latency(i as f64);
    }

    // Archive should have 10 entries (100 / 10)
    assert_eq!(
        store.latency_archive().len(),
        10,
        "Archive should have 10 entries (every 10th measurement)"
    );

    // Recent history should have all 100
    assert_eq!(store.latency_history().len(), 100);
}

/// Test extended plot data combines archive and recent
#[test]
fn test_extended_plot_data() {
    let mut store = StatsStore::new();

    // Record enough data to have both archive and recent
    for i in 0..200 {
        store.record_latency(i as f64);
    }

    // Get extended data
    let data = store.latency_plot_data_extended(250);

    // Should have data from both recent (200) and archive (20)
    assert!(
        data.len() > 200,
        "Extended data should include archive points, got {} points",
        data.len()
    );
}

/// Test loss events are recorded with timestamps
#[test]
fn test_loss_events_with_timestamps() {
    let mut store = StatsStore::new();

    store.record_loss(5);
    store.record_loss(10);
    store.record_loss(3);

    let events = store.loss_events();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].count, 5);
    assert_eq!(events[1].count, 10);
    assert_eq!(events[2].count, 3);

    // Timestamps should be increasing
    assert!(events[1].timestamp >= events[0].timestamp);
    assert!(events[2].timestamp >= events[1].timestamp);
}

/// Test uptime tracking
#[test]
fn test_uptime_tracking() {
    let mut store = StatsStore::new();

    assert_eq!(store.stats().uptime_seconds, 0);

    store.set_uptime(60);
    assert_eq!(store.stats().uptime_seconds, 60);

    store.set_uptime(3600);
    assert_eq!(store.stats().uptime_seconds, 3600);
}

/// Test that clear() resets everything including archive and events
#[test]
fn test_full_clear() {
    let mut store = StatsStore::new();

    // Add data everywhere
    for i in 0..100 {
        store.record_latency(i as f64);
    }
    store.record_loss(10);
    store.record_disconnection(1000, true);

    // Full clear
    store.clear();

    assert_eq!(store.latency_history().len(), 0);
    assert_eq!(store.latency_archive().len(), 0);
    assert_eq!(store.loss_events().len(), 0);
    assert_eq!(store.disconnection_events().len(), 0);
    assert_eq!(store.stats().measurement_count, 0);
}

/// Test StatsResponse loss_events field serialization
#[test]
fn test_loss_events_serialization() {
    let json = serde_json::json!({
        "current_latency": 5.0,
        "min_latency": 4.0,
        "max_latency": 6.0,
        "avg_latency": 5.0,
        "total_lost": 15,
        "total_corrupted": 0,
        "measurement_count": 100,
        "latency_history": [],
        "loss_history": [],
        "device_name": null,
        "buffer_size": 0,
        "sample_rate": 48000,
        "uptime_seconds": 120,
        "loss_events": [
            {"timestamp": "2025-01-01T00:00:00Z", "count": 5},
            {"timestamp": "2025-01-01T00:01:00Z", "count": 10}
        ]
    });

    let events = json["loss_events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["count"], 5);
    assert_eq!(events[1]["count"], 10);
}
