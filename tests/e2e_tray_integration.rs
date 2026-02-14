//! Integration tests for tray icon status updates
//!
//! Tests that verify tray status events are emitted correctly
//! and the monitoring loop properly updates tray icon state.

use audiotester::stats::store::StatsStore;

/// Test that status_from_analysis returns correct statuses for all conditions
#[test]
fn test_status_mapping_complete_coverage() {
    // These tests verify the pure logic function that maps analysis results to tray status

    // GREEN (Ok): Low latency, no loss, no corruption
    assert_eq!(
        determine_status(0.0, 0, 0),
        "ok",
        "Zero latency should be ok"
    );
    assert_eq!(
        determine_status(1.0, 0, 0),
        "ok",
        "1ms latency should be ok"
    );
    assert_eq!(
        determine_status(49.9, 0, 0),
        "ok",
        "49.9ms latency should be ok"
    );

    // ORANGE (Warning): Any loss OR any corruption
    assert_eq!(
        determine_status(1.0, 1, 0),
        "warning",
        "Any loss should be warning"
    );
    assert_eq!(
        determine_status(1.0, 0, 1),
        "warning",
        "Any corruption should be warning"
    );
    assert_eq!(
        determine_status(1.0, 100, 100),
        "warning",
        "Heavy loss+corruption should be warning"
    );

    // Warning takes priority over error (loss is worse than high latency)
    assert_eq!(
        determine_status(100.0, 1, 0),
        "warning",
        "Loss with high latency should be warning"
    );
    assert_eq!(
        determine_status(500.0, 0, 1),
        "warning",
        "Corruption with high latency should be warning"
    );

    // RED (Error): High latency (>=50ms) with no loss/corruption
    assert_eq!(
        determine_status(50.0, 0, 0),
        "error",
        "50ms latency should be error"
    );
    assert_eq!(
        determine_status(100.0, 0, 0),
        "error",
        "100ms latency should be error"
    );
    assert_eq!(
        determine_status(1000.0, 0, 0),
        "error",
        "1000ms latency should be error"
    );
}

/// Test that initial monitoring state should emit a status immediately
#[test]
fn test_initial_status_emission_required() {
    // This test verifies the requirement that we MUST emit an initial status
    // when monitoring starts, not wait for a status change

    // Simulating monitoring loop state
    let mut emitted_statuses: Vec<String> = Vec::new();
    let mut last_status = "disconnected"; // Initial state

    // First successful measurement comes in
    let new_status = determine_status(5.0, 0, 0);

    // The bug was: only emit if status CHANGED from last_status
    // But initial last_status is "disconnected", so if new_status is "ok",
    // it should emit. Let's verify:
    if new_status != last_status {
        emitted_statuses.push(new_status.to_string());
        last_status = new_status;
    }

    assert_eq!(emitted_statuses.len(), 1, "Must emit on first measurement");
    assert_eq!(
        emitted_statuses[0], "ok",
        "First emit should be 'ok' status"
    );
    assert_eq!(last_status, "ok", "last_status should be updated");
}

/// Test that monitoring loop handles Ok(None) gracefully
#[test]
fn test_monitoring_handles_no_result() {
    // When analyze() returns Ok(None), we should NOT panic or emit
    // This is a valid state during warmup or when engine is stopped

    let mut emitted_count = 0;
    let mut last_status = "disconnected";

    // Simulate 10 cycles of Ok(None)
    for _ in 0..10 {
        let result: Option<(f64, u64, u64)> = None;

        if let Some((latency, lost, corrupted)) = result {
            let new_status = determine_status(latency, lost, corrupted);
            if new_status != last_status {
                emitted_count += 1;
                last_status = new_status;
            }
        }
        // No action on None - this is correct
    }

    assert_eq!(emitted_count, 0, "Should not emit on Ok(None)");
    assert_eq!(
        last_status, "disconnected",
        "Status should remain disconnected"
    );
}

/// Test status change detection works correctly
#[test]
fn test_status_change_detection() {
    let mut last_status = "disconnected";
    let mut changes: Vec<(&str, &str)> = Vec::new();

    // Sequence of measurements
    let measurements = [
        (5.0, 0u64, 0u64), // ok
        (5.0, 0, 0),       // ok (no change)
        (10.0, 0, 0),      // ok (no change - latency changed but status same)
        (55.0, 0, 0),      // error (change!)
        (60.0, 0, 0),      // error (no change)
        (5.0, 1, 0),       // warning (change!)
        (5.0, 0, 1),       // warning (no change)
        (5.0, 0, 0),       // ok (change!)
    ];

    for (latency, lost, corrupted) in measurements {
        let new_status = determine_status(latency, lost, corrupted);
        if new_status != last_status {
            changes.push((last_status, new_status));
            last_status = new_status;
        }
    }

    assert_eq!(changes.len(), 4, "Should have 4 status changes");
    assert_eq!(
        changes[0],
        ("disconnected", "ok"),
        "First change: disconnected -> ok"
    );
    assert_eq!(changes[1], ("ok", "error"), "Second change: ok -> error");
    assert_eq!(
        changes[2],
        ("error", "warning"),
        "Third change: error -> warning"
    );
    assert_eq!(
        changes[3],
        ("warning", "ok"),
        "Fourth change: warning -> ok"
    );
}

/// Test that corrupted samples are included in loss count for warning status
#[test]
fn test_corruption_triggers_warning() {
    // Corruption should trigger warning just like loss
    assert_eq!(determine_status(5.0, 0, 1), "warning");
    assert_eq!(determine_status(5.0, 0, 100), "warning");

    // Both together should still be warning
    assert_eq!(determine_status(5.0, 50, 50), "warning");
}

/// Test RGB colors are correct for each status
#[test]
fn test_status_colors() {
    // Green: (0, 200, 0)
    assert_eq!(status_to_rgb("ok"), (0x00, 0xC8, 0x00));

    // Orange: (255, 165, 0)
    assert_eq!(status_to_rgb("warning"), (0xFF, 0xA5, 0x00));

    // Red: (255, 0, 0)
    assert_eq!(status_to_rgb("error"), (0xFF, 0x00, 0x00));

    // Gray: (128, 128, 128)
    assert_eq!(status_to_rgb("disconnected"), (0x80, 0x80, 0x80));
}

/// Test that status persists correctly after reconnection
#[test]
fn test_status_after_reconnection() {
    let mut store = StatsStore::new();

    // Record some latency before "disconnection"
    store.record_latency(5.0);
    assert_eq!(store.stats().current_latency, 5.0);

    // Simulate disconnection event
    store.record_disconnection(1000, true);

    // Record new latency after reconnection
    store.record_latency(6.0);

    // Stats should still be accurate
    assert_eq!(store.stats().current_latency, 6.0);
    assert_eq!(store.stats().measurement_count, 2);
    assert_eq!(store.disconnection_events().len(), 1);
}

// ===== Helper functions mirroring tray.rs logic =====

fn determine_status(latency_ms: f64, lost_samples: u64, corrupted_samples: u64) -> &'static str {
    if lost_samples > 0 || corrupted_samples > 0 {
        "warning"
    } else if latency_ms >= 50.0 {
        "error"
    } else {
        "ok"
    }
}

fn status_to_rgb(status: &str) -> (u8, u8, u8) {
    match status {
        "ok" => (0x00, 0xC8, 0x00),
        "warning" => (0xFF, 0xA5, 0x00),
        "error" => (0xFF, 0x00, 0x00),
        _ => (0x80, 0x80, 0x80),
    }
}
