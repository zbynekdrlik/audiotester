//! Tauri 2 tray icon and menu management
//!
//! Provides system tray icon with status indication and context menu.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

/// Status colors for the tray icon
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    Ok,
    Warning,
    Error,
    Disconnected,
}

/// Icon size in pixels
const ICON_SIZE: u32 = 16;

/// Set up the tray icon with menu
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let status_item = MenuItem::with_id(app, "status", "Status: Starting...", false, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let dashboard_item = MenuItem::with_id(app, "dashboard", "Open Dashboard", true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let toggle_item = MenuItem::with_id(app, "toggle", "Stop Monitoring", true, None::<&str>)?;
    let separator3 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &separator1,
            &dashboard_item,
            &separator2,
            &toggle_item,
            &separator3,
            &quit_item,
        ],
    )?;

    let icon = make_status_icon(TrayStatus::Disconnected);

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Audiotester - Audio Monitor")
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            match id {
                "dashboard" => {
                    open_dashboard(app);
                }
                "toggle" => {
                    tracing::info!("Toggle monitoring requested from tray");
                    // Toggle is handled by the main app loop
                }
                "quit" => {
                    tracing::info!("Exit requested from tray");
                    app.exit(0);
                }
                _ => {}
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

/// Create an RGBA icon for the given status
pub fn make_status_icon(status: TrayStatus) -> Image<'static> {
    let (r, g, b) = match status {
        TrayStatus::Ok => (0x00u8, 0xC8u8, 0x00u8),
        TrayStatus::Warning => (0xFF, 0xA5, 0x00),
        TrayStatus::Error => (0xFF, 0x00, 0x00),
        TrayStatus::Disconnected => (0x80, 0x80, 0x80),
    };

    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    let center = ICON_SIZE as f32 / 2.0;
    let radius = center - 1.0;

    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let dx = x as f32 - center + 0.5;
            let dy = y as f32 - center + 0.5;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                rgba.extend_from_slice(&[r, g, b, 255]);
            } else if dist <= radius + 1.0 {
                let alpha = ((radius + 1.0 - dist) * 255.0) as u8;
                rgba.extend_from_slice(&[r, g, b, alpha]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)
}
