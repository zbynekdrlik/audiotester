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
    // Set up file-based logging for test server (so /api/v1/logs works in E2E tests)
    let log_dir = std::env::temp_dir().join("audiotester-test-logs");
    std::fs::create_dir_all(&log_dir).ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("audiotester=debug".parse().unwrap()),
        )
        .init();

    // Write a marker file so /api/v1/logs can find it
    let log_file = log_dir.join("audiotester.log");
    std::fs::write(
        &log_file,
        "INFO audiotester test server starting\nINFO audiotester logging initialized\n",
    )
    .ok();

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
    let state = AppState::new(engine, Arc::clone(&stats), config, Some(log_dir));

    tracing::info!(port, "Test server starting");

    if let Err(e) = audiotester_server::start_server(state).await {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    }
}
