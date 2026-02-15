//! Tauri 2 tray icon and menu management
//!
//! Provides system tray icon with status indication and context menu.
//! Status updates are handled via Tauri global events.

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// Status colors for the tray icon
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayStatus {
    Ok,
    Warning,
    Error,
    Disconnected,
}

/// Status event payload for tray icon updates
///
/// Emitted from the monitoring loop to update tray icon color
/// based on analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayStatusEvent {
    /// Status level
    pub status: TrayStatus,
    /// Current measured latency in milliseconds
    pub latency_ms: f64,
    /// Number of lost samples in this measurement
    pub lost_samples: u64,
}

/// Icon size in pixels
const ICON_SIZE: u32 = 16;

/// Set up the tray icon with menu
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let version_label = format!(
        "v{} ({})",
        audiotester_core::VERSION,
        audiotester_core::BUILD_DATE
    );
    let version_item = MenuItem::with_id(app, "version", &version_label, false, None::<&str>)?;
    let status_item = MenuItem::with_id(app, "status", "Status: Starting...", false, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let dashboard_item = MenuItem::with_id(app, "dashboard", "Open Dashboard", true, None::<&str>)?;
    let remote_url = get_remote_url();
    let remote_item = MenuItem::with_id(
        app,
        "remote",
        format!("Remote: {}", remote_url),
        true,
        None::<&str>,
    )?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &version_item,
            &status_item,
            &separator1,
            &dashboard_item,
            &remote_item,
            &separator2,
            &quit_item,
        ],
    )?;

    let icon = make_status_icon(TrayStatus::Disconnected);

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Audiotester - Audio Monitor")
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            match id {
                "dashboard" => {
                    open_dashboard(app);
                }
                "remote" => {
                    show_remote_url(app);
                }
                "quit" => {
                    tracing::info!("Exit requested from tray");
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // Left-click opens dashboard directly (no menu)
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// Open the dashboard in the default browser
fn open_dashboard(app: &AppHandle) {
    let port = 8920;
    let url = format!("http://localhost:{}", port);
    tracing::info!("Opening dashboard: {}", url);

    // Show the Tauri window
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Get the remote access URL (IP-based for LAN access)
///
/// Returns the URL using the local IP address for easy access from
/// other devices on the network.
pub fn get_remote_url() -> String {
    let port = 8920;
    let ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| {
            hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "localhost".to_string())
        });
    format!("http://{}:{}", ip, port)
}

/// Show the remote access URL in the log (for clipboard)
fn show_remote_url(_app: &AppHandle) {
    let url = get_remote_url();
    tracing::info!("Remote access URL: {}", url);
    // In a full implementation, this would copy to clipboard
    // For now, log it so the user can see it in the console
}

/// Create an RGBA icon for the given status
///
/// Draws a 3-bar audio equalizer/level meter instead of a generic circle,
/// making the tray icon visually distinct from other applications.
pub fn make_status_icon(status: TrayStatus) -> Image<'static> {
    let (r, g, b) = match status {
        TrayStatus::Ok => (0x00u8, 0xC8u8, 0x00u8),
        TrayStatus::Warning => (0xFF, 0xA5, 0x00),
        TrayStatus::Error => (0xFF, 0x00, 0x00),
        TrayStatus::Disconnected => (0x80, 0x80, 0x80),
    };

    let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];

    // 3 vertical bars (equalizer style)
    // Bar positions: x=2..5, x=7..10, x=12..15 (3px wide each)
    // Bar heights: 10, 14, 12 pixels (varied for visual interest)
    // Bottom-aligned at y=15
    let bars: [(u32, u32, u32); 3] = [
        (2, 3, 10), // (x_start, width, height)
        (7, 3, 14),
        (12, 3, 12),
    ];
    let bottom = 15u32; // bottom pixel row

    for &(x_start, width, height) in &bars {
        let y_top = bottom.saturating_sub(height) + 1;
        for y in y_top..=bottom {
            for x in x_start..x_start + width {
                if x < ICON_SIZE && y < ICON_SIZE {
                    let idx = ((y * ICON_SIZE + x) * 4) as usize;
                    // Anti-alias the top row for rounded appearance
                    if y == y_top {
                        rgba[idx] = r;
                        rgba[idx + 1] = g;
                        rgba[idx + 2] = b;
                        rgba[idx + 3] = 180; // semi-transparent top edge
                    } else {
                        rgba[idx] = r;
                        rgba[idx + 1] = g;
                        rgba[idx + 2] = b;
                        rgba[idx + 3] = 255;
                    }
                }
            }
        }
    }

    Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)
}

/// Update the tray icon to reflect the current status
///
/// # Arguments
/// * `app` - The Tauri AppHandle
/// * `status` - The new status to display
///
/// # Returns
/// Ok(()) on success, error if tray icon cannot be updated
pub fn update_tray_status(
    app: &AppHandle,
    status: TrayStatus,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get the tray icon by id "main" (the default)
    if let Some(tray) = app.tray_by_id("main") {
        let icon = make_status_icon(status);
        tray.set_icon(Some(icon))?;

        // Update tooltip with status info
        let tooltip = match status {
            TrayStatus::Ok => "Audiotester - Monitoring OK",
            TrayStatus::Warning => "Audiotester - Warning (sample loss detected)",
            TrayStatus::Error => "Audiotester - Error (high latency)",
            TrayStatus::Disconnected => "Audiotester - Disconnected",
        };
        tray.set_tooltip(Some(tooltip))?;

        tracing::trace!("Tray icon updated to {:?}", status);
    }
    Ok(())
}

/// Determine tray status from analysis results
///
/// # Status mapping:
/// - Ok (green): Latency < 50ms, no sample loss
/// - Warning (orange): Sample loss detected
/// - Error (red): Latency >= 50ms
/// - Disconnected (gray): Not monitoring
pub fn status_from_analysis(
    latency_ms: f64,
    lost_samples: u64,
    corrupted_samples: u64,
) -> TrayStatus {
    if lost_samples > 0 || corrupted_samples > 0 {
        TrayStatus::Warning
    } else if latency_ms >= 50.0 {
        TrayStatus::Error
    } else {
        TrayStatus::Ok
    }
}
