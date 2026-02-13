//! Audiotester Tauri application
//!
//! Desktop shell providing tray icon, window, and NSIS installer.
//! All UI is served by the embedded Axum + Leptos SSR server.

pub mod tray;

use audiotester_core::stats::store::StatsStore;
use audiotester_server::{AppState, EngineHandle, ServerConfig};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Listener, Manager};

/// Global AppHandle storage for cross-thread tray updates
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

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

    // Spawn auto-configure thread if env vars are set
    if std::env::var("AUDIOTESTER_DEVICE").is_ok()
        || std::env::var("AUDIOTESTER_AUTO_START").is_ok()
    {
        let auto_engine = engine.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
            rt.block_on(async {
                auto_configure(auto_engine).await;
            });
        });
    }

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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus existing window when second instance tries to start
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
                let _ = window.show();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Store AppHandle globally for monitoring loop access
            let _ = APP_HANDLE.set(handle.clone());

            // Setup tray
            if let Err(e) = tray::setup_tray(&handle) {
                tracing::error!("Failed to setup tray: {}", e);
            }

            // Listen for tray status events from monitoring loop
            let tray_handle = handle.clone();
            handle.listen("tray-status", move |event| {
                if let Ok(payload) = serde_json::from_str::<tray::TrayStatusEvent>(event.payload())
                {
                    let status = payload.to_tray_status();
                    if let Err(e) = tray::update_tray_status(&tray_handle, status) {
                        tracing::warn!("Failed to update tray: {}", e);
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Audiotester");
}

/// Auto-configure the engine from environment variables.
///
/// Reads `AUDIOTESTER_DEVICE`, `AUDIOTESTER_SAMPLE_RATE`, and
/// `AUDIOTESTER_AUTO_START` to set up the audio engine without
/// manual web UI interaction.
async fn auto_configure(engine: EngineHandle) {
    // Wait for ASIO subsystem to initialize
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Set sample rate if specified
    if let Ok(rate_str) = std::env::var("AUDIOTESTER_SAMPLE_RATE") {
        if let Ok(rate) = rate_str.parse::<u32>() {
            tracing::info!(sample_rate = rate, "Auto-configuring sample rate");
            engine.set_sample_rate(rate).await;
        } else {
            tracing::warn!(value = %rate_str, "Invalid AUDIOTESTER_SAMPLE_RATE");
        }
    }

    // Select device with retries
    if let Ok(device_name) = std::env::var("AUDIOTESTER_DEVICE") {
        tracing::info!(device = %device_name, "Auto-configuring device");
        let mut selected = false;
        for attempt in 1..=5 {
            match engine.select_device(device_name.clone()).await {
                Ok(()) => {
                    tracing::info!(device = %device_name, attempt, "Device selected");
                    selected = true;
                    break;
                }
                Err(e) => {
                    tracing::warn!(device = %device_name, attempt, error = %e, "Device selection failed, retrying...");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        if !selected {
            tracing::error!(device = %device_name, "Failed to select device after 5 attempts");
            return;
        }
    }

    // Auto-start monitoring if requested
    if std::env::var("AUDIOTESTER_AUTO_START")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
    {
        tracing::info!("Auto-starting monitoring");
        match engine.start().await {
            Ok(()) => tracing::info!("Monitoring started successfully"),
            Err(e) => tracing::error!(error = %e, "Failed to auto-start monitoring"),
        }
    }
}

/// Main monitoring loop - analyzes audio and broadcasts stats
async fn monitoring_loop(engine: EngineHandle, stats: Arc<Mutex<StatsStore>>, state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
    let mut last_status = tray::TrayStatus::Disconnected;

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

            // Update tray icon status (only if changed to reduce overhead)
            let new_status = tray::status_from_analysis(
                result.latency_ms,
                result.lost_samples as u64,
                result.corrupted_samples as u64,
            );

            if new_status != last_status {
                last_status = new_status;

                // Emit tray status event if app handle is available
                if let Some(app) = APP_HANDLE.get() {
                    let event = tray::TrayStatusEvent {
                        status: match new_status {
                            tray::TrayStatus::Ok => "ok".to_string(),
                            tray::TrayStatus::Warning => "warning".to_string(),
                            tray::TrayStatus::Error => "error".to_string(),
                            tray::TrayStatus::Disconnected => "disconnected".to_string(),
                        },
                        latency_ms: result.latency_ms,
                        lost_samples: result.lost_samples as u64,
                    };

                    if let Err(e) = app.emit("tray-status", event) {
                        tracing::warn!("Failed to emit tray status event: {}", e);
                    }
                }
            }
        }
    }
}
