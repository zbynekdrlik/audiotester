//! Settings page - Leptos SSR
//!
//! Device selection, sample rate configuration, and monitoring controls.

use super::{SETTINGS_SCRIPT, SETTINGS_STYLES};
use crate::AppState;
use axum::extract::State;
use axum::response::Html;
use leptos::prelude::*;
use reactive_graph::owner::Owner;

/// Settings page component
#[component]
fn SettingsPage() -> impl IntoView {
    view! {
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
                <meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self' ws: wss:; img-src 'self' data:;"/>
                <link rel="icon" type="image/x-icon" href="/favicon.ico"/>
                <title>"Audiotester - Settings"</title>
                <style>{SETTINGS_STYLES}</style>
            </head>
            <body>
                <header class="header">
                    <h1>"Audiotester"</h1>
                    <nav>
                        <a href="/" class="nav-link">"Dashboard"</a>
                        <a href="/settings" class="nav-link active">"Settings"</a>
                    </nav>
                </header>
                <main>
                    <section class="settings-section">
                        <h2>"Audio Device"</h2>
                        <div class="form-group">
                            <label for="device-select">"Device"</label>
                            <select id="device-select" aria-label="Device">
                                <option value="">"Loading devices..."</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label for="sample-rate">"Sample Rate"</label>
                            <select id="sample-rate" aria-label="Sample Rate">
                                <option value="44100">"44100 Hz"</option>
                                <option value="48000">"48000 Hz"</option>
                                <option value="88200">"88200 Hz"</option>
                                <option value="96000" selected="selected">"96000 Hz"</option>
                                <option value="176400">"176400 Hz"</option>
                                <option value="192000">"192000 Hz"</option>
                            </select>
                        </div>
                    </section>
                    <section class="settings-section">
                        <h2>"Channel Pair"</h2>
                        <p class="channel-pair-description">"Select which channels carry the test signal and frame counter (1-based)."</p>
                        <div class="channel-pair-row">
                            <div class="form-group">
                                <label for="signal-channel">"Signal Channel"</label>
                                <select id="signal-channel" class="channel-select" aria-label="Signal Channel">
                                    <option value="1">"1"</option>
                                </select>
                            </div>
                            <div class="form-group">
                                <label for="counter-channel">"Counter Channel"</label>
                                <select id="counter-channel" class="channel-select" aria-label="Counter Channel">
                                    <option value="2">"2"</option>
                                </select>
                            </div>
                        </div>
                        <div id="channel-error" class="channel-error" style="display:none"></div>
                    </section>
                    <section class="settings-section">
                        <h2>"Monitoring"</h2>
                        <div class="monitoring-controls">
                            <div class="status-display" id="monitoring-status">"Stopped"</div>
                            <button id="start-btn" class="btn btn-primary">"Start"</button>
                            <button id="stop-btn" class="btn btn-danger" disabled="disabled">"Stop"</button>
                        </div>
                    </section>
                    <section class="settings-section">
                        <h2>"Device Information"</h2>
                        <div id="device-info" class="device-info">
                            <p>"Select a device to see details."</p>
                        </div>
                    </section>
                </main>
                <script>{SETTINGS_SCRIPT}</script>
            </body>
        </html>
    }
}

/// Axum handler for the settings page
pub async fn settings_page(State(_state): State<AppState>) -> Html<String> {
    let owner = Owner::new_root(None);
    let html = owner.with(|| view! { <SettingsPage/> }.into_view().to_html());
    Html(format!("<!DOCTYPE html>{html}"))
}
