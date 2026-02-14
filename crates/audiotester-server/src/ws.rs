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
    let loss_events: Vec<crate::api::LossEventResponse> = store
        .loss_events()
        .iter()
        .rev()
        .take(100)
        .map(|e| crate::api::LossEventResponse {
            timestamp: e.timestamp.to_rfc3339(),
            count: e.count,
        })
        .collect();
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
        // Device info from cached stats (updated by monitoring loop)
        device_name: stats.device_name,
        buffer_size: stats.buffer_size,
        sample_rate: stats.sample_rate,
        uptime_seconds: stats.uptime_seconds,
        loss_events,
        samples_sent: stats.samples_sent,
        samples_received: stats.samples_received,
        signal_lost: stats.signal_lost,
        confidence: stats.last_confidence,
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

    // Use oneshot for graceful shutdown instead of abort()
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn task to forward broadcast messages to this client
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(msg) => {
                            if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                _ = &mut shutdown_rx => break,
            }
        }
    });

    // Spawn task to handle incoming messages (pings, close)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => { let _ = shutdown_tx.send(()); }
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
