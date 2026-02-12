//! Statistics window with real-time graphs
//!
//! Displays latency measurements, sample loss events, and other
//! metrics in a dedicated GUI window using egui.

use crate::stats::store::StatsStore;

/// Statistics window state
pub struct StatsWindow {
    /// Whether the window is visible
    visible: bool,
    /// Reference to the stats store
    store: Option<std::sync::Arc<std::sync::Mutex<StatsStore>>>,
}

impl StatsWindow {
    /// Create a new statistics window
    pub fn new() -> Self {
        Self {
            visible: false,
            store: None,
        }
    }

    /// Set the stats store reference
    pub fn set_store(&mut self, store: std::sync::Arc<std::sync::Mutex<StatsStore>>) {
        self.store = Some(store);
    }

    /// Show the window
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the window
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Toggle window visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Check if window is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Render the window (called from egui context)
    ///
    /// # Arguments
    /// * `ctx` - egui context for rendering
    pub fn render(&mut self, ctx: &egui::Context) {
        if !self.visible {
            return;
        }

        egui::Window::new("Audiotester Statistics")
            .default_size([600.0, 400.0])
            .show(ctx, |ui| {
                self.render_content(ui);
            });
    }

    /// Render window content
    fn render_content(&self, ui: &mut egui::Ui) {
        // Header with current status
        ui.heading("Audio Path Monitor");
        ui.separator();

        // Current metrics
        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.colored_label(egui::Color32::GREEN, "OK");

            ui.separator();

            ui.label("Latency:");
            ui.label("-- ms");

            ui.separator();

            ui.label("Lost:");
            ui.label("0");

            ui.separator();

            ui.label("Corrupted:");
            ui.label("0");
        });

        ui.separator();

        // Latency graph placeholder
        ui.collapsing("Latency History", |ui| {
            // TODO: Phase 5 - Add egui_plot chart
            ui.label("Latency graph will appear here");

            // Placeholder for plot
            let plot_height = 150.0;
            ui.allocate_space(egui::vec2(ui.available_width(), plot_height));
        });

        // Sample loss graph placeholder
        ui.collapsing("Sample Loss Events", |ui| {
            // TODO: Phase 5 - Add egui_plot chart
            ui.label("Sample loss graph will appear here");

            let plot_height = 150.0;
            ui.allocate_space(egui::vec2(ui.available_width(), plot_height));
        });

        ui.separator();

        // Device info
        ui.collapsing("Device Information", |ui| {
            ui.horizontal(|ui| {
                ui.label("Device:");
                ui.label("Not selected");
            });
            ui.horizontal(|ui| {
                ui.label("Sample Rate:");
                ui.label("48000 Hz");
            });
            ui.horizontal(|ui| {
                ui.label("Buffer Size:");
                ui.label("-- samples");
            });
        });
    }
}

impl Default for StatsWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_visibility() {
        let mut window = StatsWindow::new();
        assert!(!window.is_visible());

        window.show();
        assert!(window.is_visible());

        window.hide();
        assert!(!window.is_visible());

        window.toggle();
        assert!(window.is_visible());

        window.toggle();
        assert!(!window.is_visible());
    }
}
