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
    let mut last_device_name: Option<String> = None;
    let mut device_info_update_counter: u32 = 0;
    let mut last_successful_analysis: Option<std::time::Instant> = None;
    let mut signal_lost = false;
    let mut signal_lost_since: Option<std::time::Instant> = None;
    let mut signal_recovery_attempts: u32 = 0;

    // Wait for Tauri APP_HANDLE to be available (setup happens in parallel)
    while APP_HANDLE.get().is_none() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tracing::info!("APP_HANDLE available, starting monitoring");

    // Emit initial disconnected status so tray shows gray at startup
    emit_tray_status(tray::TrayStatus::Disconnected, 0.0, 0);

    loop {
        interval.tick().await;

        // Update uptime and device info periodically (every 10 cycles = 1 second)
        device_info_update_counter += 1;
        if device_info_update_counter >= 10 {
            device_info_update_counter = 0;

            // Get engine status and cache in stats store
            if let Ok(engine_status) = engine.get_status().await {
                if let Ok(mut store) = stats.lock() {
                    store.set_uptime(start_time.elapsed().as_secs());
                    store.set_device_info(
                        engine_status.device_name.clone(),
                        engine_status.sample_rate,
                        0, // Buffer size not exposed by cpal yet
                    );
                }

                // Track if device changed (for reconnection)
                if engine_status.device_name != last_device_name {
                    if last_device_name.is_some() {
                        tracing::info!(
                            old = ?last_device_name,
                            new = ?engine_status.device_name,
                            "Device changed"
                        );
                    }
                    last_device_name = engine_status.device_name;
                }
            }

            // Update sample counters from engine (cumulative values)
            if let Ok((sent, received)) = engine.get_sample_counts().await {
                if let Ok(mut store) = stats.lock() {
                    store.set_samples_sent(sent as u64);
                    store.set_samples_received(received as u64);
                }
            }
        }

        // Try to analyze
        match engine.analyze().await {
            Ok(Some(result)) => {
                // Check if signal is valid:
                // 1. Latency must be in valid range (1-100ms for loopback)
                //    - >100ms indicates MLS period aliasing (no real correlation peak)
                // 2. Confidence must be above threshold
                let latency_valid = result.latency_ms > 0.0 && result.latency_ms < 100.0;
                let confidence_valid = result.confidence >= 0.3;
                let has_valid_signal = latency_valid && confidence_valid;

                if has_valid_signal {
                    // Update last successful analysis time only for valid signals
                    last_successful_analysis = Some(std::time::Instant::now());

                    // Reset signal_lost if it was set
                    if signal_lost {
                        signal_lost = false;
                        signal_lost_since = None;
                        signal_recovery_attempts = 0;
                        if let Ok(mut store) = stats.lock() {
                            store.set_signal_lost(false);
                        }
                        tracing::info!(
                            "Signal restored (latency: {:.2}ms, confidence: {:.2})",
                            result.latency_ms,
                            result.confidence
                        );
                    }
                } else if !signal_lost {
                    // Invalid signal - set signal_lost immediately
                    signal_lost = true;
                    signal_lost_since = Some(std::time::Instant::now());
                    if let Ok(mut store) = stats.lock() {
                        store.set_signal_lost(true);
                    }
                    tracing::warn!(
                        "No signal detected (latency: {:.2}ms, confidence: {:.2})",
                        result.latency_ms,
                        result.confidence
                    );
                }

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
                    store.set_confidence(result.confidence);
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
                    tracing::debug!(status = ?new_status, "Tray status changed");
                }
            }
            Ok(None) => {
                // No result yet (engine might be stopped or warming up)
                // Check for signal timeout (1 second without analysis result while engine running)
                if let Ok(status) = engine.get_status().await {
                    if status.state == audiotester_core::audio::engine::EngineState::Running {
                        if let Some(last) = last_successful_analysis {
                            if last.elapsed() > Duration::from_secs(1) && !signal_lost {
                                signal_lost = true;
                                signal_lost_since = Some(std::time::Instant::now());
                                if let Ok(mut store) = stats.lock() {
                                    store.set_signal_lost(true);
                                }
                                tracing::warn!("No signal detected (analysis timeout)");
                            }
                        }
                    }
                }
                // Still broadcast stats so dashboard updates
                audiotester_server::ws::broadcast_stats(&state);
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

                    // FULL reconnection: stop, re-select device, start
                    // This handles buffer size changes in ASIO driver
                    if let Err(stop_err) = engine.stop().await {
                        tracing::debug!(error = %stop_err, "Stop during reconnect (may be expected)");
                    }

                    // Re-select the same device to reinitialize ASIO
                    if let Some(ref device) = last_device_name {
                        if let Err(select_err) = engine.select_device(device.clone()).await {
                            tracing::warn!(
                                device = %device,
                                error = %select_err,
                                "Failed to re-select device during reconnect"
                            );
                        }
                    }

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

        // Auto-recovery: restart ASIO stream after sustained signal loss.
        // VBMatrix routing changes (mute/unmute) don't affect a running ASIO
        // stream. Simple stop + wait + start (NO device re-select) picks up
        // routing changes. Re-selecting the device would try to open a new
        // ASIO handle while the old one isn't fully released, causing failure.
        if signal_lost && !reconnect_in_progress {
            if let Some(lost_at) = signal_lost_since {
                let lost_duration = lost_at.elapsed();
                // First attempt after 5s, then every 15s, up to 5 attempts
                let next_attempt_at =
                    Duration::from_secs(5) + Duration::from_secs(15) * signal_recovery_attempts;
                if lost_duration >= next_attempt_at
                    && signal_recovery_attempts < MAX_RECONNECT_ATTEMPTS
                {
                    signal_recovery_attempts += 1;
                    tracing::info!(
                        attempt = signal_recovery_attempts,
                        lost_secs = lost_duration.as_secs(),
                        "Signal lost - restarting ASIO stream for auto-recovery"
                    );

                    // Stop the engine (releases ASIO streams)
                    if let Err(e) = engine.stop().await {
                        tracing::debug!(error = %e, "Stop during signal recovery");
                    }

                    // Wait for ASIO driver to release streams
                    tokio::time::sleep(Duration::from_millis(500)).await;

                    // Start the engine (device is already selected, just
                    // creates new streams - same as manual monitoring restart)
                    match engine.start().await {
                        Ok(()) => {
                            tracing::info!(
                                attempt = signal_recovery_attempts,
                                "ASIO stream restarted for signal recovery"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                attempt = signal_recovery_attempts,
                                error = %e,
                                "Failed to restart ASIO stream for signal recovery"
                            );
                        }
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

        tracing::debug!(status = ?status, "Emitting tray status event");

        if let Err(e) = app.emit("tray-status", event) {
            tracing::warn!("Failed to emit tray status event: {}", e);
        }
    } else {
        tracing::trace!("APP_HANDLE not yet available, skipping tray emit");
    }
}
