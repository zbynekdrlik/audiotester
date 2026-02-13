//! WebSocket handler for real-time stats push
//!
//! Clients connect to /api/v1/ws to receive live statistics updates.

use crate::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};

/// WebSocket upgrade handler
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

/// Build a stats JSON snapshot (must not hold lock across await)
fn build_stats_json(state: &AppState) -> Option<String> {
    let store = state.stats.lock().ok()?;
    let stats = store.stats().clone();
    let latency_history = store.latency_plot_data(300);
    let loss_history = store.loss_plot_data(300);
    drop(store);

    let response = crate::api::StatsResponse {
        current_latency: stats.current_latency,
        min_latency: if stats.min_latency == f64::MAX {
            0.0
        } else {
            stats.min_latency
        },
        max_latency: stats.max_latency,
        avg_latency: stats.avg_latency,
        total_lost: stats.total_lost,
        total_corrupted: stats.total_corrupted,
        measurement_count: stats.measurement_count,
        latency_history,
        loss_history,
    };
    serde_json::to_string(&response).ok()
}

/// Handle an individual WebSocket connection
async fn handle_ws(socket: WebSocket, state: AppState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Send initial stats snapshot (lock is dropped before await)
    if let Some(json) = build_stats_json(&state) {
        let _ = ws_sender.send(Message::Text(json.into())).await;
    }

    // Subscribe to broadcast channel
    let mut rx = state.ws_tx.subscribe();

    // Spawn task to forward broadcast messages to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Spawn task to handle incoming messages (pings, close)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }

    tracing::debug!("WebSocket client disconnected");
}

/// Broadcast stats update to all connected WebSocket clients.
/// Called from the monitoring loop.
pub fn broadcast_stats(state: &AppState) {
    if state.ws_tx.receiver_count() == 0 {
        return;
    }

    if let Some(json) = build_stats_json(state) {
        let _ = state.ws_tx.send(json);
    }
}
