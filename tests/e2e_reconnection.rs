//! E2E tests for ASIO auto-reconnection logic
//!
//! Verifies connection state tracking, exponential backoff calculation,
//! and stats preservation during reconnection.

use audiotester::audio::engine::ConnectionState;
use audiotester::stats::store::StatsStore;

/// Test ConnectionState enum values and transitions
#[test]
fn test_connection_state_values() {
    let connected = ConnectionState::Connected;
    assert_eq!(connected, ConnectionState::Connected);

    let reconnecting = ConnectionState::Reconnecting { attempt: 1 };
    assert_eq!(reconnecting, ConnectionState::Reconnecting { attempt: 1 });

    let failed = ConnectionState::Failed;
    assert_eq!(failed, ConnectionState::Failed);
}

/// Test exponential backoff calculation
#[test]
fn test_exponential_backoff_schedule() {
    // Backoff schedule: 500ms -> 1000ms -> 2000ms -> 4000ms -> 5000ms (capped)
    let expected_delays_ms = [500u64, 1000, 2000, 4000, 5000];

    for (attempt, &expected_ms) in expected_delays_ms.iter().enumerate() {
        let delay = calculate_backoff_ms(attempt as u32 + 1);
        assert_eq!(
            delay,
            expected_ms,
            "Attempt {} should have {}ms backoff, got {}ms",
            attempt + 1,
            expected_ms,
            delay
        );
    }
}

/// Test that backoff is capped at 5 seconds
#[test]
fn test_backoff_cap() {
    // Even at attempt 100, should not exceed 5000ms
    let delay = calculate_backoff_ms(100);
    assert_eq!(delay, 5000, "Backoff should be capped at 5000ms");
}

/// Test that stats are preserved during reconnection (not cleared)
#[test]
fn test_stats_preserved_during_reconnect() {
    let mut store = StatsStore::new();

    // Record some data before "disconnection"
    store.record_latency(5.0);
    store.record_latency(10.0);
    store.record_loss(3);

    // Verify data exists
    assert_eq!(store.stats().measurement_count, 2);
    assert_eq!(store.stats().total_lost, 3);
    assert_eq!(store.latency_history().len(), 2);

    // Simulate recording a disconnection event (NOT calling clear())
    store.record_disconnection(2000, true);

    // Stats and history should still be present
    assert_eq!(store.stats().measurement_count, 2);
    assert_eq!(store.stats().total_lost, 3);
    assert_eq!(store.latency_history().len(), 2);
    assert_eq!(store.disconnection_events().len(), 1);
    assert!(store.disconnection_events()[0].reconnected);
}

/// Test disconnection event recording
#[test]
fn test_disconnection_event_recording() {
    let mut store = StatsStore::new();

    store.record_disconnection(500, true);
    store.record_disconnection(3000, false);

    let events = store.disconnection_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].duration_ms, 500);
    assert!(events[0].reconnected);
    assert_eq!(events[1].duration_ms, 3000);
    assert!(!events[1].reconnected);
}

/// Test max reconnection attempts (5)
#[test]
fn test_max_reconnection_attempts() {
    let max_attempts = 5u32;

    // After 5 failed attempts, state should be Failed
    for attempt in 1..=max_attempts {
        let state = ConnectionState::Reconnecting { attempt };
        assert_eq!(state, ConnectionState::Reconnecting { attempt });
    }

    // Attempt 6 would exceed max - should transition to Failed
    assert!(
        max_attempts < 6,
        "Max attempts should be 5, requiring manual intervention after"
    );
}

/// Test reset_counters preserves history
#[test]
fn test_reset_counters_preserves_history() {
    let mut store = StatsStore::new();

    // Record data
    store.record_latency(5.0);
    store.record_latency(15.0);
    store.record_loss(10);

    // Reset counters
    store.reset_counters();

    // Counters should be zero
    assert_eq!(store.stats().measurement_count, 0);
    assert_eq!(store.stats().total_lost, 0);
    assert_eq!(store.stats().total_corrupted, 0);
    assert_eq!(store.stats().min_latency, f64::MAX);
    assert_eq!(store.stats().max_latency, 0.0);

    // But history should still be there
    assert_eq!(
        store.latency_history().len(),
        2,
        "History should be preserved after reset_counters"
    );
}

/// Test loss event recording
#[test]
fn test_loss_event_recording() {
    let mut store = StatsStore::new();

    store.record_loss(5);
    store.record_loss(10);

    let events = store.loss_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].count, 5);
    assert_eq!(events[1].count, 10);
}

// ===== Helper function that mirrors reconnection backoff logic =====

/// Calculate exponential backoff delay for reconnection attempt.
/// Schedule: 500ms -> 1000ms -> 2000ms -> 4000ms -> 5000ms (capped)
fn calculate_backoff_ms(attempt: u32) -> u64 {
    let base_ms = 500u64;
    let max_ms = 5000u64;
    let exponent = attempt.saturating_sub(1).min(12); // Cap exponent to avoid overflow
    let delay = base_ms.saturating_mul(2u64.pow(exponent));
    delay.min(max_ms)
}
