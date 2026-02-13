//! Audiotester Tauri application
//!
//! Desktop shell providing tray icon, window, and NSIS installer.
//! All UI is served by the embedded Axum + Leptos SSR server.

pub mod tray;

use audiotester_core::stats::store::StatsStore;
use audiotester_server::{AppState, EngineHandle, ServerConfig};
use std::sync::{Arc, Mutex};

/// Run the Tauri application
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("audiotester=info".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting Audiotester v{}", audiotester_core::VERSION);

    // Create shared state
    let engine = EngineHandle::spawn();
    let stats = Arc::new(Mutex::new(StatsStore::new()));

    let config = ServerConfig::default();
    let state = AppState::new(engine.clone(), Arc::clone(&stats), config);

    // Spawn the web server
    let server_state = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async {
            if let Err(e) = audiotester_server::start_server(server_state).await {
                tracing::error!("Web server error: {}", e);
            }
        });
    });

    // Spawn the monitoring loop
    let monitor_state = state.clone();
    let monitor_engine = engine;
    let monitor_stats = stats;
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async {
            monitoring_loop(monitor_engine, monitor_stats, monitor_state).await;
        });
    });

    // Build and run Tauri app
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let handle = app.handle().clone();
            if let Err(e) = tray::setup_tray(&handle) {
                tracing::error!("Failed to setup tray: {}", e);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Audiotester");
}

/// Main monitoring loop - analyzes audio and broadcasts stats
async fn monitoring_loop(engine: EngineHandle, stats: Arc<Mutex<StatsStore>>, state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        interval.tick().await;

        // Try to analyze
        if let Ok(Some(result)) = engine.analyze().await {
            // Record to stats store
            if let Ok(mut store) = stats.lock() {
                store.record_latency(result.latency_ms);
                if result.lost_samples > 0 {
                    store.record_loss(result.lost_samples as u64);
                }
                if result.corrupted_samples > 0 {
                    store.record_corruption(result.corrupted_samples as u64);
                }
            }

            // Broadcast to WebSocket clients
            audiotester_server::ws::broadcast_stats(&state);
        }
    }
}
