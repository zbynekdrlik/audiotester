//! System tray icon and menu management
//!
//! Provides a system tray icon with status indication and context menu
//! for device selection and application control.

use thiserror::Error;
use tray_icon::menu::MenuEvent;
#[cfg(windows)]
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
#[cfg(windows)]
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Icon size in pixels
#[cfg(windows)]
const ICON_SIZE: u32 = 16;

/// Status colors for the tray icon
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    /// Everything OK - green icon
    Ok,
    /// Warning (high latency, etc.) - orange icon
    Warning,
    /// Error (lost samples, no signal) - red icon
    Error,
    /// Disconnected/stopped - gray icon
    Disconnected,
}

/// Errors that can occur with tray operations
#[derive(Error, Debug)]
pub enum TrayError {
    #[error("Failed to create tray icon: {0}")]
    CreationFailed(String),

    #[error("Failed to update tray icon: {0}")]
    UpdateFailed(String),

    #[error("Failed to create menu: {0}")]
    MenuError(String),
}

/// Menu action from tray context menu
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    /// Open statistics window
    ShowStats,
    /// Open device selection dialog
    SelectDevice,
    /// Start/stop monitoring
    ToggleMonitoring,
    /// Exit application
    Exit,
}

/// Menu item IDs
#[cfg(windows)]
const MENU_ID_STATUS: &str = "status";
const MENU_ID_SHOW_STATS: &str = "show_stats";
const MENU_ID_SELECT_DEVICE: &str = "select_device";
const MENU_ID_TOGGLE: &str = "toggle";
const MENU_ID_EXIT: &str = "exit";

/// System tray manager
pub struct TrayManager {
    status: TrayStatus,
    is_monitoring: bool,
    #[cfg(windows)]
    tray_icon: Option<TrayIcon>,
    #[cfg(windows)]
    status_item: Option<MenuItem>,
    #[cfg(windows)]
    toggle_item: Option<MenuItem>,
}

impl TrayManager {
    /// Create a new tray manager
    ///
    /// # Returns
    /// Result containing the tray manager or an error
    pub fn new() -> Result<Self, TrayError> {
        Ok(Self {
            status: TrayStatus::Disconnected,
            is_monitoring: false,
            #[cfg(windows)]
            tray_icon: None,
            #[cfg(windows)]
            status_item: None,
            #[cfg(windows)]
            toggle_item: None,
        })
    }

    /// Initialize the tray icon (must be called from main thread on Windows)
    #[cfg(windows)]
    pub fn init(&mut self, device_name: Option<&str>) -> Result<(), TrayError> {
        // Create menu items
        let status_text = match device_name {
            Some(name) => format!("Device: {}", name),
            None => "Status: Disconnected".to_string(),
        };
        let status_item = MenuItem::with_id(MENU_ID_STATUS, &status_text, false, None);
        let show_stats_item = MenuItem::with_id(MENU_ID_SHOW_STATS, "Show Statistics", true, None);
        let select_device_item =
            MenuItem::with_id(MENU_ID_SELECT_DEVICE, "Select Device...", true, None);
        let toggle_item = MenuItem::with_id(MENU_ID_TOGGLE, "Start Monitoring", true, None);
        let exit_item = MenuItem::with_id(MENU_ID_EXIT, "Exit", true, None);

        // Store references
        self.status_item = Some(status_item.clone());
        self.toggle_item = Some(toggle_item.clone());

        // Build menu
        let menu = Menu::with_items(&[
            &status_item,
            &PredefinedMenuItem::separator(),
            &show_stats_item,
            &select_device_item,
            &PredefinedMenuItem::separator(),
            &toggle_item,
            &PredefinedMenuItem::separator(),
            &exit_item,
        ])
        .map_err(|e| TrayError::MenuError(e.to_string()))?;

        // Create icon
        let icon = Self::create_icon(self.status);

        // Build tray icon
        let tray = TrayIconBuilder::new()
            .with_tooltip("Audiotester - Audio Monitor")
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .build()
            .map_err(|e| TrayError::CreationFailed(e.to_string()))?;

        self.tray_icon = Some(tray);
        Ok(())
    }

    /// Initialize the tray icon (non-Windows stub)
    #[cfg(not(windows))]
    pub fn init(&mut self, _device_name: Option<&str>) -> Result<(), TrayError> {
        // Tray icons are Windows-only in this application
        Ok(())
    }

    /// Create an RGBA icon for the given status
    #[cfg(windows)]
    fn create_icon(status: TrayStatus) -> Icon {
        let (r, g, b) = match status {
            TrayStatus::Ok => (0x00, 0xC8, 0x00),           // Green
            TrayStatus::Warning => (0xFF, 0xA5, 0x00),      // Orange
            TrayStatus::Error => (0xFF, 0x00, 0x00),        // Red
            TrayStatus::Disconnected => (0x80, 0x80, 0x80), // Gray
        };

        // Create a simple filled circle icon
        let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
        let center = ICON_SIZE as f32 / 2.0;
        let radius = center - 1.0;

        for y in 0..ICON_SIZE {
            for x in 0..ICON_SIZE {
                let dx = x as f32 - center + 0.5;
                let dy = y as f32 - center + 0.5;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist <= radius {
                    // Inside circle - use status color
                    rgba.push(r);
                    rgba.push(g);
                    rgba.push(b);
                    rgba.push(255); // Fully opaque
                } else if dist <= radius + 1.0 {
                    // Anti-aliased edge
                    let alpha = ((radius + 1.0 - dist) * 255.0) as u8;
                    rgba.push(r);
                    rgba.push(g);
                    rgba.push(b);
                    rgba.push(alpha);
                } else {
                    // Outside circle - transparent
                    rgba.push(0);
                    rgba.push(0);
                    rgba.push(0);
                    rgba.push(0);
                }
            }
        }

        Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).expect("Failed to create icon")
    }

    /// Update the tray icon status
    ///
    /// # Arguments
    /// * `status` - New status to display
    pub fn set_status(&mut self, status: TrayStatus) -> Result<(), TrayError> {
        self.status = status;

        #[cfg(windows)]
        if let Some(ref tray) = self.tray_icon {
            let icon = Self::create_icon(status);
            tray.set_icon(Some(icon))
                .map_err(|e| TrayError::UpdateFailed(e.to_string()))?;
        }

        Ok(())
    }

    /// Get current status
    pub fn status(&self) -> TrayStatus {
        self.status
    }

    /// Set monitoring state
    pub fn set_monitoring(&mut self, monitoring: bool) {
        self.is_monitoring = monitoring;
        if !monitoring {
            self.status = TrayStatus::Disconnected;
        }

        // Update toggle menu item text
        #[cfg(windows)]
        if let Some(ref toggle_item) = self.toggle_item {
            let text = if monitoring {
                "Stop Monitoring"
            } else {
                "Start Monitoring"
            };
            toggle_item.set_text(text);
        }
    }

    /// Check if monitoring is active
    pub fn is_monitoring(&self) -> bool {
        self.is_monitoring
    }

    /// Set tooltip text
    ///
    /// # Arguments
    /// * `text` - Tooltip text to display on hover
    pub fn set_tooltip(&mut self, text: &str) -> Result<(), TrayError> {
        #[cfg(windows)]
        if let Some(ref tray) = self.tray_icon {
            tray.set_tooltip(Some(text))
                .map_err(|e| TrayError::UpdateFailed(e.to_string()))?;
        }
        let _ = text; // Suppress unused warning on non-Windows
        Ok(())
    }

    /// Update status display text
    pub fn set_status_text(&mut self, text: &str) -> Result<(), TrayError> {
        #[cfg(windows)]
        if let Some(ref status_item) = self.status_item {
            status_item.set_text(text);
        }
        let _ = text;
        Ok(())
    }

    /// Poll for menu events (non-blocking)
    ///
    /// # Returns
    /// Optional action if a menu item was clicked
    pub fn poll_event(&self) -> Option<TrayAction> {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            return match event.id.0.as_str() {
                MENU_ID_SHOW_STATS => Some(TrayAction::ShowStats),
                MENU_ID_SELECT_DEVICE => Some(TrayAction::SelectDevice),
                MENU_ID_TOGGLE => Some(TrayAction::ToggleMonitoring),
                MENU_ID_EXIT => Some(TrayAction::Exit),
                _ => None,
            };
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_creation() {
        let tray = TrayManager::new().unwrap();
        assert_eq!(tray.status(), TrayStatus::Disconnected);
        assert!(!tray.is_monitoring());
    }

    #[test]
    fn test_status_update() {
        let mut tray = TrayManager::new().unwrap();

        tray.set_status(TrayStatus::Ok).unwrap();
        assert_eq!(tray.status(), TrayStatus::Ok);

        tray.set_status(TrayStatus::Warning).unwrap();
        assert_eq!(tray.status(), TrayStatus::Warning);

        tray.set_status(TrayStatus::Error).unwrap();
        assert_eq!(tray.status(), TrayStatus::Error);
    }

    #[test]
    fn test_monitoring_state() {
        let mut tray = TrayManager::new().unwrap();

        tray.set_monitoring(true);
        assert!(tray.is_monitoring());

        tray.set_monitoring(false);
        assert!(!tray.is_monitoring());
        assert_eq!(tray.status(), TrayStatus::Disconnected);
    }

    #[cfg(windows)]
    #[test]
    fn test_icon_creation() {
        // Test that icons can be created for all statuses
        let _ok = TrayManager::create_icon(TrayStatus::Ok);
        let _warn = TrayManager::create_icon(TrayStatus::Warning);
        let _err = TrayManager::create_icon(TrayStatus::Error);
        let _disc = TrayManager::create_icon(TrayStatus::Disconnected);
    }
}
