//! Leptos SSR UI components
//!
//! Server-rendered HTML pages with embedded JavaScript for interactivity.

pub mod components;
pub mod dashboard;
pub mod settings;

/// CSS styles for the dashboard
pub const DASHBOARD_STYLES: &str = include_str!("styles/dashboard.css");

/// CSS styles for the settings page
pub const SETTINGS_STYLES: &str = include_str!("styles/settings.css");

/// JavaScript for dashboard interactivity (charts + WebSocket)
pub const DASHBOARD_SCRIPT: &str = include_str!("scripts/dashboard.js");

/// JavaScript for settings page interactivity
pub const SETTINGS_SCRIPT: &str = include_str!("scripts/settings.js");

/// TradingView Lightweight Charts v5.1 standalone production build
pub const LIGHTWEIGHT_CHARTS_SCRIPT: &str =
    include_str!("scripts/lightweight-charts.standalone.production.js");

/// PWA manifest JSON
pub const MANIFEST_JSON: &str = include_str!("manifest.json");

/// Favicon for browser tab identification
pub const FAVICON_ICO: &[u8] = include_bytes!("icons/favicon.ico");

/// Escape </script> tags in embedded content
pub fn escape_script_tag(s: &str) -> String {
    s.replace("</script>", r#"<\/script>"#)
}
