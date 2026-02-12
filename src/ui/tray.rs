//! System tray icon and menu management
//!
//! Provides a system tray icon with status indication and context menu
//! for device selection and application control.

use thiserror::Error;

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

/// System tray manager
pub struct TrayManager {
    status: TrayStatus,
    is_monitoring: bool,
}

impl TrayManager {
    /// Create a new tray manager
    ///
    /// # Returns
    /// Result containing the tray manager or an error
    pub fn new() -> Result<Self, TrayError> {
        // TODO: Phase 4 - Initialize tray-icon
        Ok(Self {
            status: TrayStatus::Disconnected,
            is_monitoring: false,
        })
    }

    /// Update the tray icon status
    ///
    /// # Arguments
    /// * `status` - New status to display
    pub fn set_status(&mut self, status: TrayStatus) -> Result<(), TrayError> {
        // TODO: Phase 4 - Update icon color based on status
        self.status = status;
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
    }

    /// Check if monitoring is active
    pub fn is_monitoring(&self) -> bool {
        self.is_monitoring
    }

    /// Set tooltip text
    ///
    /// # Arguments
    /// * `text` - Tooltip text to display on hover
    pub fn set_tooltip(&mut self, _text: &str) -> Result<(), TrayError> {
        // TODO: Phase 4 - Update tooltip
        Ok(())
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
}
