//! Statistics window with real-time graphs
//!
//! Displays latency measurements, sample loss events, and other
//! metrics in a dedicated GUI window using egui and egui_plot.

use crate::stats::store::StatsStore;
use std::sync::{Arc, Mutex};

/// Number of data points to show in graphs
const PLOT_POINTS: usize = 300;

/// Statistics window eframe application
pub struct StatsApp {
    /// Shared reference to the stats store
    store: Arc<Mutex<StatsStore>>,
    /// Device name for display
    device_name: String,
    /// Sample rate for display
    sample_rate: u32,
}

impl StatsApp {
    /// Create a new stats application
    pub fn new(store: Arc<Mutex<StatsStore>>, device_name: String, sample_rate: u32) -> Self {
        Self {
            store,
            device_name,
            sample_rate,
        }
    }

    /// Render the current stats summary bar
    fn render_summary(&self, ui: &mut egui::Ui) {
        let store = match self.store.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let stats = store.stats().clone();
        drop(store);

        ui.horizontal(|ui| {
            // Current latency
            ui.label("Latency:");
            if stats.measurement_count > 0 {
                let color = if stats.current_latency < 10.0 {
                    egui::Color32::from_rgb(0x00, 0xC8, 0x00)
                } else if stats.current_latency < 50.0 {
                    egui::Color32::from_rgb(0xFF, 0xA5, 0x00)
                } else {
                    egui::Color32::from_rgb(0xFF, 0x00, 0x00)
                };
                ui.colored_label(color, format!("{:.2} ms", stats.current_latency));
            } else {
                ui.label("-- ms");
            }

            ui.separator();

            // Min/Max/Avg
            if stats.measurement_count > 0 {
                ui.label(format!(
                    "Min: {:.2} | Max: {:.2} | Avg: {:.2} ms",
                    stats.min_latency, stats.max_latency, stats.avg_latency
                ));
            }

            ui.separator();

            // Loss / Corruption
            let lost_color = if stats.total_lost > 0 {
                egui::Color32::from_rgb(0xFF, 0x00, 0x00)
            } else {
                egui::Color32::from_rgb(0x00, 0xC8, 0x00)
            };
            ui.label("Lost:");
            ui.colored_label(lost_color, format!("{}", stats.total_lost));

            ui.separator();

            ui.label("Corrupted:");
            let corrupt_color = if stats.total_corrupted > 0 {
                egui::Color32::from_rgb(0xFF, 0x00, 0x00)
            } else {
                egui::Color32::from_rgb(0x00, 0xC8, 0x00)
            };
            ui.colored_label(corrupt_color, format!("{}", stats.total_corrupted));

            ui.separator();

            ui.label(format!("Samples: {}", stats.measurement_count));
        });
    }

    /// Render latency history plot
    fn render_latency_plot(&self, ui: &mut egui::Ui) {
        let store = match self.store.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let data = store.latency_plot_data(PLOT_POINTS);
        let stats = store.stats().clone();
        drop(store);

        let points: egui_plot::PlotPoints = data.iter().map(|&(t, v)| [t, v]).collect();

        let line = egui_plot::Line::new(points)
            .color(egui::Color32::from_rgb(0x00, 0xA0, 0xFF))
            .name("Latency (ms)");

        egui_plot::Plot::new("latency_plot")
            .height(180.0)
            .x_axis_label("Time (seconds ago)")
            .y_axis_label("Latency (ms)")
            .legend(egui_plot::Legend::default())
            .show(ui, |plot_ui| {
                plot_ui.line(line);

                // Show average as horizontal line if we have data
                if stats.measurement_count > 0 {
                    let avg_line = egui_plot::HLine::new(stats.avg_latency)
                        .color(egui::Color32::from_rgb(0xFF, 0xFF, 0x00).gamma_multiply(0.5))
                        .name(format!("Avg: {:.2} ms", stats.avg_latency));
                    plot_ui.hline(avg_line);
                }
            });
    }

    /// Render sample loss plot
    fn render_loss_plot(&self, ui: &mut egui::Ui) {
        let store = match self.store.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let data = store.loss_plot_data(PLOT_POINTS);
        drop(store);

        let bars: Vec<egui_plot::Bar> = data
            .iter()
            .map(|&(t, v)| egui_plot::Bar::new(t, v).width(0.8))
            .collect();

        let chart = egui_plot::BarChart::new(bars)
            .color(egui::Color32::from_rgb(0xFF, 0x40, 0x40))
            .name("Lost samples");

        egui_plot::Plot::new("loss_plot")
            .height(120.0)
            .x_axis_label("Time (seconds ago)")
            .y_axis_label("Lost samples")
            .legend(egui_plot::Legend::default())
            .show(ui, |plot_ui| {
                plot_ui.bar_chart(chart);
            });
    }

    /// Render device information section
    fn render_device_info(&self, ui: &mut egui::Ui) {
        egui::Grid::new("device_info_grid")
            .num_columns(2)
            .spacing([20.0, 4.0])
            .show(ui, |ui| {
                ui.label("Device:");
                ui.label(&self.device_name);
                ui.end_row();

                ui.label("Sample Rate:");
                ui.label(format!("{} Hz", self.sample_rate));
                ui.end_row();
            });
    }
}

impl eframe::App for StatsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request repaint every 500ms for live data
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Audiotester Statistics");
            ui.separator();

            // Summary bar
            self.render_summary(ui);
            ui.separator();

            // Latency graph
            ui.collapsing("Latency History", |ui| {
                self.render_latency_plot(ui);
            })
            .header_response
            .on_hover_text("Latency measurements over time");

            // Loss graph
            ui.collapsing("Sample Loss Events", |ui| {
                self.render_loss_plot(ui);
            })
            .header_response
            .on_hover_text("Detected sample loss events over time");

            ui.separator();

            // Device info
            ui.collapsing("Device Information", |ui| {
                self.render_device_info(ui);
            });
        });
    }
}

/// Spawn the statistics window in a separate thread
///
/// # Arguments
/// * `store` - Shared stats store for reading data
/// * `device_name` - Name of the audio device being monitored
/// * `sample_rate` - Current sample rate in Hz
pub fn open_stats_window(store: Arc<Mutex<StatsStore>>, device_name: String, sample_rate: u32) {
    std::thread::spawn(move || {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([700.0, 500.0])
                .with_min_inner_size([400.0, 300.0]),
            ..Default::default()
        };

        if let Err(e) = eframe::run_native(
            "Audiotester Statistics",
            options,
            Box::new(move |_cc| Ok(Box::new(StatsApp::new(store, device_name, sample_rate)))),
        ) {
            tracing::error!("Failed to open statistics window: {}", e);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_stats_window_types() {
        // Verify the types compile correctly
        let store = Arc::new(Mutex::new(StatsStore::new()));
        let _app = StatsApp::new(store, "Test Device".to_string(), 96000);
    }
}
