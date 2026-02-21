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
use super::{DASHBOARD_SCRIPT, DASHBOARD_STYLES, LIGHTWEIGHT_CHARTS_SCRIPT};
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
                <meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self' ws: wss:; img-src 'self' data:;"/>
                <link rel="manifest" href="/manifest.json"/>
                <link rel="icon" type="image/x-icon" href="/favicon.ico"/>
                <title>"Audiotester - Dashboard"</title>
                <style>{DASHBOARD_STYLES}</style>
            </head>
            <body>
                <header class="header">
                    <h1>"Audiotester"</h1>
                    <span class="version-info" id="version-info" data-testid="version-info"></span>
                    <nav>
                        <a href="/" class="nav-link active">"Dashboard"</a>
                        <a href="/settings" class="nav-link">"Settings"</a>
                    </nav>
                    <span
                        class="remote-url"
                        id="remote-url"
                        data-testid="remote-url"
                        title="Click to copy"
                    ></span>
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
                            <span class="info-label">"Rate:"</span>
                            <span class="info-value" id="sample-rate-display">"--"</span>
                        </div>
                        <div class="info-item">
                            <span class="info-label">"Uptime:"</span>
                            <span class="info-value" id="uptime-display">"--"</span>
                        </div>
                        <div class="info-item samples-counter">
                            <span class="info-label">"OUT:"</span>
                            <span class="info-value" id="samples-sent">"0"</span>
                        </div>
                        <div class="info-item samples-counter">
                            <span class="info-label">"IN:"</span>
                            <span class="info-value" id="samples-received">"0"</span>
                        </div>
                        <div class="info-item">
                            <span class="info-label">"TX:"</span>
                            <span class="info-value" id="tx-rate">"--"</span>
                        </div>
                        <div class="info-item">
                            <span class="info-label">"RX:"</span>
                            <span class="info-value" id="rx-rate">"--"</span>
                        </div>
                        <span
                            class="signal-status"
                            id="signal-status"
                            data-testid="signal-status"
                        >"Signal OK"</span>
                        <button class="btn-reset" id="reset-btn" title="Reset counters (preserves graph history)">"Reset"</button>
                    </div>
                    <section class="charts">
                        <div class="chart-container">
                            <div class="chart-header">
                                <h2>"Sample Loss Timeline"</h2>
                                <div class="zoom-controls" id="loss-zoom-controls">
                                    <button class="zoom-btn" data-range="10m">"10m"</button>
                                    <button class="zoom-btn active" data-range="1h">"1h"</button>
                                    <button class="zoom-btn" data-range="6h">"6h"</button>
                                    <button class="zoom-btn" data-range="12h">"12h"</button>
                                    <button class="zoom-btn" data-range="24h">"24h"</button>
                                    <button class="zoom-btn" data-range="3d">"3d"</button>
                                    <button class="zoom-btn" data-range="7d">"7d"</button>
                                    <button class="zoom-btn" data-range="14d">"14d"</button>
                                </div>
                                <button class="zoom-btn live active" id="loss-live-btn">"Live"</button>
                            </div>
                            <div id="loss-timeline" class="chart" data-testid="loss-timeline"></div>
                        </div>
                        <div class="chart-container">
                            <div class="chart-header">
                                <h2>"Latency Timeline"</h2>
                                <div class="zoom-controls" id="latency-zoom-controls">
                                    <button class="zoom-btn" data-range="10m">"10m"</button>
                                    <button class="zoom-btn active" data-range="1h">"1h"</button>
                                    <button class="zoom-btn" data-range="6h">"6h"</button>
                                    <button class="zoom-btn" data-range="12h">"12h"</button>
                                    <button class="zoom-btn" data-range="24h">"24h"</button>
                                    <button class="zoom-btn" data-range="3d">"3d"</button>
                                    <button class="zoom-btn" data-range="7d">"7d"</button>
                                    <button class="zoom-btn" data-range="14d">"14d"</button>
                                </div>
                                <button class="zoom-btn live active" id="latency-live-btn">"Live"</button>
                            </div>
                            <div id="latency-chart" class="chart" data-testid="latency-timeline"></div>
                        </div>
                    </section>
                </main>
                <script>{LIGHTWEIGHT_CHARTS_SCRIPT}</script>
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
