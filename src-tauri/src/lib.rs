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

/// Calculate exponential backoff delay for reconnection.
/// Schedule: 500ms -> 1000ms -> 2000ms -> 4000ms -> 5000ms (capped)
fn calculate_backoff_ms(attempt: u32) -> u64 {
    let base_ms = 500u64;
    let max_ms = 5000u64;
    let exponent = attempt.saturating_sub(1).min(12); // Cap exponent to avoid overflow
    let delay = base_ms.saturating_mul(2u64.pow(exponent));
    delay.min(max_ms)
}

/// Maximum number of reconnection attempts before requiring manual intervention
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

/// Main monitoring loop - analyzes audio and broadcasts stats
///
/// Includes auto-reconnection with exponential backoff. When the audio engine
/// encounters an error, it will attempt to reconnect up to MAX_RECONNECT_ATTEMPTS
/// times with exponential backoff. Stats and graph history are preserved during
/// reconnection (no clear() is called).
async fn monitoring_loop(engine: EngineHandle, stats: Arc<Mutex<StatsStore>>, state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
    let mut last_status = tray::TrayStatus::Disconnected;
    let mut consecutive_failures: u32 = 0;
    let mut reconnect_in_progress = false;
    let start_time = std::time::Instant::now();

    loop {
        interval.tick().await;

        // Update uptime
        if let Ok(mut store) = stats.lock() {
            store.set_uptime(start_time.elapsed().as_secs());
        }

        // Try to analyze
        match engine.analyze().await {
            Ok(Some(result)) => {
                // Reset failure counter on successful analysis
                if consecutive_failures > 0 {
                    tracing::info!(
                        "Audio engine recovered after {} failed attempts",
                        consecutive_failures
                    );

                    // Record successful reconnection
                    if reconnect_in_progress {
                        if let Ok(mut store) = stats.lock() {
                            store.record_disconnection((consecutive_failures as u64) * 500, true);
                        }
                        reconnect_in_progress = false;
                    }
                }
                consecutive_failures = 0;

                // Record to stats store (preserve existing data - no clear!)
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
                    emit_tray_status(new_status, result.latency_ms, result.lost_samples as u64);
                }
            }
            Ok(None) => {
                // No result yet (engine might be stopped or warming up)
            }
            Err(e) => {
                // Engine error - attempt reconnection
                consecutive_failures += 1;
                reconnect_in_progress = true;

                if consecutive_failures <= MAX_RECONNECT_ATTEMPTS {
                    let backoff = calculate_backoff_ms(consecutive_failures);
                    tracing::warn!(
                        attempt = consecutive_failures,
                        max = MAX_RECONNECT_ATTEMPTS,
                        backoff_ms = backoff,
                        error = %e,
                        "Audio engine error, attempting reconnection"
                    );

                    // Update tray to disconnected
                    if last_status != tray::TrayStatus::Disconnected {
                        last_status = tray::TrayStatus::Disconnected;
                        emit_tray_status(tray::TrayStatus::Disconnected, 0.0, 0);
                    }

                    // Wait with exponential backoff before next attempt
                    tokio::time::sleep(Duration::from_millis(backoff)).await;

                    // Try to restart the engine
                    match engine.start().await {
                        Ok(()) => {
                            tracing::info!(
                                attempt = consecutive_failures,
                                "Audio engine reconnected successfully"
                            );
                        }
                        Err(restart_err) => {
                            tracing::error!(
                                attempt = consecutive_failures,
                                error = %restart_err,
                                "Failed to restart audio engine"
                            );
                        }
                    }
                } else if consecutive_failures == MAX_RECONNECT_ATTEMPTS + 1 {
                    // Only log once when max attempts exceeded
                    tracing::error!(
                        "Max reconnection attempts ({}) exceeded. Manual intervention required.",
                        MAX_RECONNECT_ATTEMPTS
                    );

                    // Record failed reconnection
                    if let Ok(mut store) = stats.lock() {
                        store.record_disconnection((MAX_RECONNECT_ATTEMPTS as u64) * 5000, false);
                    }
                }
            }
        }
    }
}

/// Emit a tray status event to update the system tray icon
fn emit_tray_status(status: tray::TrayStatus, latency_ms: f64, lost_samples: u64) {
    if let Some(app) = APP_HANDLE.get() {
        let event = tray::TrayStatusEvent {
            status: match status {
                tray::TrayStatus::Ok => "ok".to_string(),
                tray::TrayStatus::Warning => "warning".to_string(),
                tray::TrayStatus::Error => "error".to_string(),
                tray::TrayStatus::Disconnected => "disconnected".to_string(),
            },
            latency_ms,
            lost_samples,
        };

        if let Err(e) = app.emit("tray-status", event) {
            tracing::warn!("Failed to emit tray status event: {}", e);
        }
    }
}
