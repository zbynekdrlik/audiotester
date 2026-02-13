//! Standalone test server for E2E testing
//!
//! Starts the Axum server with a real engine thread but no ASIO requirement.
//! On systems without ASIO, the engine will return empty device lists and
//! analyze() returns None — but all API and UI endpoints work.

use audiotester_core::stats::store::StatsStore;
use audiotester_server::{AppState, EngineHandle, ServerConfig};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("audiotester=debug".parse().unwrap()),
        )
        .init();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8920u16);

    let engine = EngineHandle::spawn();
    let stats = Arc::new(Mutex::new(StatsStore::new()));

    let config = ServerConfig {
        port,
        bind_addr: "127.0.0.1".to_string(),
    };
    let state = AppState::new(engine, Arc::clone(&stats), config);

    tracing::info!(port, "Test server starting");

    if let Err(e) = audiotester_server::start_server(state).await {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    }
}
