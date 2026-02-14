//! Dashboard page - Leptos SSR
//!
//! Main statistics dashboard showing real-time latency and loss data.
//! Features:
//! - Summary bar with current/min/max/avg latency and loss counters
//! - Device info bar showing active device, sample rate, uptime
//! - Reset button for counters (preserves graph history)
//! - Flexbox no-scroll layout (height: 100vh)
//! - PWA-ready meta tags

use super::components::summary_bar::SummaryBar;
use super::{DASHBOARD_SCRIPT, DASHBOARD_STYLES};
use crate::AppState;
use axum::extract::State;
use axum::response::Html;
use leptos::prelude::*;
use reactive_graph::owner::Owner;

/// Dashboard page component
#[component]
fn DashboardPage() -> impl IntoView {
    view! {
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1.0, user-scalable=no"/>
                <meta name="apple-mobile-web-app-capable" content="yes"/>
                <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent"/>
                <meta name="theme-color" content="#1a1a2e"/>
                <link rel="manifest" href="/manifest.json"/>
                <title>"Audiotester - Dashboard"</title>
                <style>{DASHBOARD_STYLES}</style>
            </head>
            <body>
                <header class="header">
                    <h1>"Audiotester"</h1>
                    <nav>
                        <a href="/" class="nav-link active">"Dashboard"</a>
                        <a href="/settings" class="nav-link">"Settings"</a>
                    </nav>
                    <div class="status-indicator" id="connection-status">"Connecting..."</div>
                </header>
                <main>
                    <SummaryBar/>
                    <div class="device-info-bar" id="device-info-bar">
                        <div class="info-item">
                            <span class="info-label">"Device:"</span>
                            <span class="info-value" id="device-name">"--"</span>
                        </div>
                        <div class="info-item">
                            <span class="info-label">"Sample Rate:"</span>
                            <span class="info-value" id="sample-rate-display">"--"</span>
                        </div>
                        <div class="info-item">
                            <span class="info-label">"Uptime:"</span>
                            <span class="info-value" id="uptime-display">"--"</span>
                        </div>
                        <button class="btn-reset" id="reset-btn" title="Reset counters (preserves graph history)">"Reset"</button>
                    </div>
                    <section class="charts">
                        <div class="chart-container">
                            <h2>"Latency History"</h2>
                            <div id="latency-chart" class="chart"></div>
                        </div>
                        <div class="chart-container">
                            <h2>"Sample Loss Events"</h2>
                            <div id="loss-chart" class="chart"></div>
                        </div>
                    </section>
                </main>
                <script>{DASHBOARD_SCRIPT}</script>
            </body>
        </html>
    }
}

/// Axum handler for the dashboard page
pub async fn dashboard_page(State(_state): State<AppState>) -> Html<String> {
    let owner = Owner::new_root(None);
    let html = owner.with(|| view! { <DashboardPage/> }.into_view().to_html());
    Html(format!("<!DOCTYPE html>{html}"))
}
