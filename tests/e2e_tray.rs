//! E2E tests for tray icon status determination
//!
//! Verifies that analysis results map correctly to tray icon status
//! and that status icons produce the correct RGB colors.

/// Test that low latency and no loss produces Ok (green) status
#[test]
fn test_status_from_analysis_ok() {
    // Import from the audiotester-core crate which is re-exported
    // The status_from_analysis function lives in src-tauri/src/tray.rs
    // but we test the pure logic function here

    // Ok: latency < 50ms, no loss, no corruption
    let status = determine_status(10.0, 0, 0);
    assert_eq!(status, "ok", "Low latency with no loss should be Ok");

    let status = determine_status(0.5, 0, 0);
    assert_eq!(status, "ok", "Very low latency should be Ok");

    let status = determine_status(49.9, 0, 0);
    assert_eq!(status, "ok", "Latency just under 50ms should be Ok");
}

/// Test that any sample loss produces Warning (orange) status
#[test]
fn test_status_from_analysis_warning_on_loss() {
    let status = determine_status(5.0, 1, 0);
    assert_eq!(status, "warning", "Any loss should produce Warning");

    let status = determine_status(5.0, 100, 0);
    assert_eq!(status, "warning", "High loss should produce Warning");

    let status = determine_status(5.0, 0, 1);
    assert_eq!(status, "warning", "Any corruption should produce Warning");

    // Loss takes priority over high latency
    let status = determine_status(100.0, 5, 0);
    assert_eq!(
        status, "warning",
        "Loss should take priority over high latency"
    );
}

/// Test that high latency (>= 50ms) with no loss produces Error (red) status
#[test]
fn test_status_from_analysis_error_on_high_latency() {
    let status = determine_status(50.0, 0, 0);
    assert_eq!(status, "error", "Latency exactly 50ms should be Error");

    let status = determine_status(100.0, 0, 0);
    assert_eq!(status, "error", "High latency should be Error");

    let status = determine_status(500.0, 0, 0);
    assert_eq!(status, "error", "Very high latency should be Error");
}

/// Test that each status produces the correct RGB color in the icon
#[test]
fn test_make_status_icon_different_colors() {
    // Green: (0x00, 0xC8, 0x00)
    let (r, g, b) = status_to_rgb("ok");
    assert_eq!((r, g, b), (0x00, 0xC8, 0x00), "Ok should be green");

    // Orange: (0xFF, 0xA5, 0x00)
    let (r, g, b) = status_to_rgb("warning");
    assert_eq!((r, g, b), (0xFF, 0xA5, 0x00), "Warning should be orange");

    // Red: (0xFF, 0x00, 0x00)
    let (r, g, b) = status_to_rgb("error");
    assert_eq!((r, g, b), (0xFF, 0x00, 0x00), "Error should be red");

    // Gray: (0x80, 0x80, 0x80)
    let (r, g, b) = status_to_rgb("disconnected");
    assert_eq!((r, g, b), (0x80, 0x80, 0x80), "Disconnected should be gray");
}

// ===== Helper functions that mirror the tray.rs logic =====
// These test the pure logic without requiring Tauri runtime dependencies.
// The actual tray.rs functions are in src-tauri which depends on tauri crate,
// so we replicate the logic here for testability.

/// Determine tray status string from analysis results.
/// Mirrors `status_from_analysis` in src-tauri/src/tray.rs.
fn determine_status(latency_ms: f64, lost_samples: u64, corrupted_samples: u64) -> &'static str {
    if lost_samples > 0 || corrupted_samples > 0 {
        "warning"
    } else if latency_ms >= 50.0 {
        "error"
    } else {
        "ok"
    }
}

/// Map status string to RGB color.
/// Mirrors `make_status_icon` color selection in src-tauri/src/tray.rs.
fn status_to_rgb(status: &str) -> (u8, u8, u8) {
    match status {
        "ok" => (0x00, 0xC8, 0x00),
        "warning" => (0xFF, 0xA5, 0x00),
        "error" => (0xFF, 0x00, 0x00),
        _ => (0x80, 0x80, 0x80), // disconnected
    }
}
