//! Summary bar component showing current metrics

use leptos::prelude::*;

/// Summary bar showing current latency, lost, and corrupted counts
#[component]
pub fn SummaryBar() -> impl IntoView {
    view! {
        <div class="summary-bar" id="summary-bar">
            <div class="metric">
                <span class="metric-label">"Latency"</span>
                <span class="metric-value" data-testid="latency-value">"--"</span>
                <span class="metric-unit">"ms"</span>
            </div>
            <div class="metric">
                <span class="metric-label">"Lost"</span>
                <span class="metric-value" data-testid="lost-value">"0"</span>
            </div>
            <div class="metric">
                <span class="metric-label">"Corrupted"</span>
                <span class="metric-value" data-testid="corrupted-value">"0"</span>
            </div>
        </div>
    }
}
