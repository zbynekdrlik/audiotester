//! Audiotester Tauri application
//!
//! Desktop shell providing tray icon, window, and NSIS installer.
//! All UI is served by the embedded Axum + Leptos SSR server.

pub mod tray;

use audiotester_core::stats::store::StatsStore;
use audiotester_server::{AppState, EngineHandle, ServerConfig};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Listener, Manager, WindowEvent};

/// Global AppHandle storage for cross-thread tray updates
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Notify when APP_HANDLE becomes available (replaces busy-wait polling)
static APP_HANDLE_NOTIFY: OnceLock<Arc<tokio::sync::Notify>> = OnceLock::new();

/// Run the Tauri application
pub fn run() {
    // Set panic handler for better diagnostics
    std::panic::set_hook(Box::new(|info| {
        tracing::error!("PANIC: {}", info);
    }));

    // Initialize file-based logging with daily rotation
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("audiotester")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(&log_dir, "audiotester.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("audiotester=debug".parse().unwrap())
        .add_directive("audiotester_core=debug".parse().unwrap())
        .add_directive("audiotester_server=info".parse().unwrap());

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .init();

    tracing::info!(log_dir = %log_dir.display(), "Logging initialized");
    tracing::info!("Starting Audiotester v{}", audiotester_core::VERSION);

    // Set process priority to HIGH for audio stability (prevents ASIO callback starvation
    // when other windows overlap the dashboard - issue #23)
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, SetPriorityClass, HIGH_PRIORITY_CLASS,
        };
        let result = unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) };
        if result != 0 {
            tracing::info!("Process priority set to HIGH for audio stability");
        } else {
            tracing::warn!("Failed to set process priority to HIGH");
        }
    }

    // Initialize the Notify before spawning any tasks
    let _ = APP_HANDLE_NOTIFY.set(Arc::new(tokio::sync::Notify::new()));

    // Create shared state
    let engine = EngineHandle::spawn();
    let stats = Arc::new(Mutex::new(StatsStore::new()));

    let config = ServerConfig::default();
    let state = AppState::new(engine.clone(), Arc::clone(&stats), config, Some(log_dir));

    // Single Tokio runtime for all async tasks
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let rt_handle = rt.handle().clone();

    // Spawn the web server
    let server_state = state.clone();
    rt_handle.spawn(async move {
        if let Err(e) = audiotester_server::start_server(server_state).await {
            tracing::error!("Web server error: {}", e);
        }
    });

    // Spawn auto-configure if env vars are set
    if std::env::var("AUDIOTESTER_DEVICE").is_ok()
        || std::env::var("AUDIOTESTER_AUTO_START").is_ok()
    {
        let auto_engine = engine.clone();
        rt_handle.spawn(async move {
            auto_configure(auto_engine).await;
        });
    }

    // Spawn the monitoring loop
    let monitor_state = state.clone();
    let monitor_engine = engine;
    let monitor_stats = stats;
    rt_handle.spawn(async move {
        monitoring_loop(monitor_engine, monitor_stats, monitor_state).await;
    });

    // Keep runtime alive in a background thread (Tauri owns the main thread)
    std::thread::spawn(move || {
        rt.block_on(std::future::pending::<()>());
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
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();

            // Store AppHandle globally for monitoring loop access
            let _ = APP_HANDLE.set(handle.clone());
            // Notify waiting tasks that APP_HANDLE is available
            if let Some(notify) = APP_HANDLE_NOTIFY.get() {
                notify.notify_waiters();
            }

            // Setup tray
            if let Err(e) = tray::setup_tray(&handle) {
                tracing::error!("Failed to setup tray: {}", e);
            }

            // Listen for tray status events from monitoring loop
            let tray_handle = handle.clone();
            handle.listen("tray-status", move |event| {
                if let Ok(payload) = serde_json::from_str::<tray::TrayStatusEvent>(event.payload())
                {
                    if let Err(e) = tray::update_tray_status(&tray_handle, payload.status) {
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
    // Wait for ASIO subsystem to initialize after boot/reboot.
    // VBMatrix may take 30-60s to fully start after Windows login.
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Set sample rate if specified (trim to handle batch file whitespace)
    if let Ok(rate_str) = std::env::var("AUDIOTESTER_SAMPLE_RATE") {
        let trimmed = rate_str.trim();
        if let Ok(rate) = trimmed.parse::<u32>() {
            tracing::info!(sample_rate = rate, "Auto-configuring sample rate");
            engine.set_sample_rate(rate).await;
        } else {
            tracing::warn!(value = %rate_str, "Invalid AUDIOTESTER_SAMPLE_RATE");
        }
    }

    let device_name = std::env::var("AUDIOTESTER_DEVICE").ok();
    let auto_start = std::env::var("AUDIOTESTER_AUTO_START")
        .map(|v| v.trim() == "true" || v.trim() == "1")
        .unwrap_or(false);

    if let Some(ref device_name) = device_name {
        tracing::info!(device = %device_name, "Auto-configuring device");

        // Select device and start monitoring with retries
        // After reboot, ASIO drivers may need time to fully initialize,
        // so we retry the full select+start cycle
        for attempt in 1..=20 {
            // Re-select device each attempt (fresh ASIO host handle)
            match engine.select_device(device_name.clone()).await {
                Ok(()) => {
                    tracing::info!(device = %device_name, attempt, "Device selected");

                    if auto_start {
                        match engine.start().await {
                            Ok(()) => {
                                tracing::info!(attempt, "Monitoring started successfully");
                                return;
                            }
                            Err(e) => {
                                tracing::warn!(attempt, error = %e, "Monitoring start failed, will retry...");
                                // Stop to clean up any partial state
                                let _ = engine.stop().await;
                            }
                        }
                    } else {
                        return;
                    }
                }
                Err(e) => {
                    tracing::warn!(device = %device_name, attempt, error = %e, "Device selection failed, retrying...");
                }
            }

            // 5s between each attempt, total retry window ~110s
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        tracing::error!(device = %device_name, "Failed to auto-configure after 20 attempts");
    } else if auto_start {
        tracing::info!("Auto-starting monitoring (no device specified)");
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

/// Lost samples threshold for detecting virtual ASIO driver restarts (issue #26).
/// When VBMatrix restarts its audio engine, VASIO-8 does NOT send
/// kAsioResetRequest. Instead, ~14000-17000 samples are lost in one cycle.
/// Detection triggers a full engine restart: stop ASIO → wait for VBMatrix
/// to settle → reconnect fresh. A fresh ASIO connection to a settled
/// VBMatrix always produces consistent buffer phase alignment.
const ASIO_RESTART_LOST_THRESHOLD: usize = 5000;

/// Delay after stopping ASIO before reconnecting (issue #26).
/// VBMatrix needs several seconds to fully restart its audio engine.
/// Reconnecting too early gives non-deterministic buffer phase (±128 samples).
/// 5s is conservative: VBMatrix typically settles in 2-3s after restart.
const ASIO_RESTART_SETTLE_MS: u64 = 5000;

/// Phase drift threshold in milliseconds (issue #26).
/// ASIO double-buffering causes the buffer phase to toggle on each fresh
/// connection.  After reconnecting we compare the new latency to the
/// pre-restart value.  If the difference exceeds this threshold we do one
/// extra reconnect to toggle the phase back.  At 96 kHz / 128-frame buffer
/// the phase shift is 1.333 ms, so 0.8 ms catches it reliably without
/// false-triggering on normal jitter (±0.02 ms).
const PHASE_DRIFT_THRESHOLD_MS: f64 = 0.8;

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
    let mut reconnect_start: Option<std::time::Instant> = None;
    // Phase verification state for ASIO restart recovery (issue #26).
    // ASIO double-buffering toggles the buffer phase on each fresh connection.
    // We save the pre-restart latency and verify the post-reconnect phase.
    let mut pre_restart_latency: Option<f64> = None;
    let mut asio_restart_in_progress = false;
    let mut valid_measurement_seen = false;

    // Wait for Tauri APP_HANDLE to be available (event-driven, no polling)
    if APP_HANDLE.get().is_none() {
        if let Some(notify) = APP_HANDLE_NOTIFY.get() {
            notify.notified().await;
        }
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

        // Check for ASIO stream invalidation (issue #26):
        // cpal 0.17 fires StreamError::StreamInvalidated when the ASIO driver
        // sends kAsioResetRequest (e.g. VBMatrix "Restart Audio Engine").
        // When detected, do a full engine restart for clean measurement state.
        if let Ok(true) = engine.is_stream_invalidated().await {
            tracing::warn!("ASIO stream invalidated (driver reset detected), restarting engine");

            // Full engine restart: stop → re-select device → start
            if let Err(e) = engine.stop().await {
                tracing::debug!(error = %e, "Stop during stream invalidation recovery");
            }

            // Brief pause for ASIO driver to settle
            tokio::time::sleep(Duration::from_millis(500)).await;

            if let Some(ref device) = last_device_name {
                if let Err(e) = engine.select_device(device.clone()).await {
                    tracing::warn!(error = %e, "Failed to re-select device after stream invalidation");
                }
            }

            match engine.start().await {
                Ok(()) => {
                    tracing::info!("Engine restarted after ASIO stream invalidation");
                    last_successful_analysis = None;
                    signal_lost = false;
                    signal_lost_since = None;
                    if let Ok(mut store) = stats.lock() {
                        store.set_signal_lost(false);
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to restart engine after stream invalidation");
                }
            }
            continue;
        }

        // Try to analyze
        match engine.analyze().await {
            Ok(Some(result)) => {
                // Detect virtual ASIO driver restart via lost samples (issue #26).
                // VASIO-8 doesn't send kAsioResetRequest, but VBMatrix restart
                // causes ~14000-17000 lost samples in a single cycle.
                // Skip on initial startup (first few cycles often show lost
                // samples before the analyzer has established a baseline).
                if result.lost_samples > ASIO_RESTART_LOST_THRESHOLD && valid_measurement_seen {
                    // Save pre-restart latency for phase verification.
                    // ASIO double-buffering toggles the buffer phase on each
                    // fresh connection (±128 samples = ±1.33ms at 96kHz).
                    // After reconnecting we compare to this value; if the phase
                    // drifted we do one extra reconnect to toggle it back.
                    pre_restart_latency = Some(result.latency_ms);
                    asio_restart_in_progress = true;

                    tracing::warn!(
                        lost_samples = result.lost_samples,
                        latency_ms = %format!("{:.3}", result.latency_ms),
                        "ASIO driver restart detected (lost samples), reconnecting"
                    );

                    // Step 1: Release ASIO driver immediately
                    if let Err(e) = engine.stop().await {
                        tracing::debug!(error = %e, "Stop during ASIO restart recovery");
                    }

                    // Step 2: Wait for VBMatrix to fully settle.
                    tracing::info!(
                        settle_ms = ASIO_RESTART_SETTLE_MS,
                        "Waiting for ASIO driver to settle before reconnecting"
                    );
                    tokio::time::sleep(Duration::from_millis(ASIO_RESTART_SETTLE_MS)).await;

                    // Step 3: Fresh ASIO connection
                    if let Some(ref device) = last_device_name {
                        if let Err(e) = engine.select_device(device.clone()).await {
                            tracing::warn!(error = %e, "Failed to re-select device after ASIO restart");
                        }
                    }

                    match engine.start().await {
                        Ok(()) => {
                            tracing::info!(
                                "Engine reconnected after ASIO driver restart, verifying phase"
                            );
                            last_successful_analysis = None;
                            signal_lost = false;
                            signal_lost_since = None;
                            if let Ok(mut store) = stats.lock() {
                                store.set_signal_lost(false);
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to reconnect after ASIO driver restart");
                            asio_restart_in_progress = false;
                            pre_restart_latency = None;
                        }
                    }
                    continue;
                }

                // Check if signal is valid:
                // 1. Latency must be in valid range (1-100ms for loopback)
                //    - >100ms indicates MLS period aliasing (no real correlation peak)
                // 2. Confidence must be above threshold
                let latency_valid = result.latency_ms > 0.0 && result.latency_ms < 100.0;
                let confidence_valid = result.confidence >= 0.3;
                let has_valid_signal = latency_valid && confidence_valid;

                if has_valid_signal {
                    // Mark that we've seen a valid measurement (skip false
                    // positives on initial startup).
                    valid_measurement_seen = true;

                    // Update last successful analysis time only for valid signals
                    last_successful_analysis = Some(std::time::Instant::now());

                    // Phase verification after ASIO restart recovery (issue #26).
                    // ASIO double-buffering toggles the buffer phase on each
                    // fresh connection.  Compare first post-reconnect latency to
                    // the pre-restart value.  If it drifted by more than the
                    // threshold, do one extra reconnect to toggle the phase back.
                    if asio_restart_in_progress {
                        asio_restart_in_progress = false;
                        if let Some(old_lat) = pre_restart_latency.take() {
                            let drift = (result.latency_ms - old_lat).abs();
                            if drift > PHASE_DRIFT_THRESHOLD_MS {
                                tracing::warn!(
                                    old_latency_ms = %format!("{:.3}", old_lat),
                                    new_latency_ms = %format!("{:.3}", result.latency_ms),
                                    drift_ms = %format!("{:.3}", drift),
                                    "Phase drift detected after ASIO reconnect, toggling phase"
                                );

                                // Quick reconnect to toggle the buffer phase.
                                // VBMatrix is already settled, just need to cycle
                                // the ASIO connection.
                                if let Err(e) = engine.stop().await {
                                    tracing::debug!(error = %e, "Stop during phase toggle");
                                }
                                tokio::time::sleep(Duration::from_millis(500)).await;
                                if let Some(ref device) = last_device_name {
                                    if let Err(e) = engine.select_device(device.clone()).await {
                                        tracing::warn!(error = %e, "Failed to re-select device during phase toggle");
                                    }
                                }
                                match engine.start().await {
                                    Ok(()) => {
                                        tracing::info!("Engine reconnected after phase toggle");
                                        last_successful_analysis = None;
                                        signal_lost = false;
                                        signal_lost_since = None;
                                        if let Ok(mut store) = stats.lock() {
                                            store.set_signal_lost(false);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "Failed to reconnect during phase toggle");
                                    }
                                }
                                continue;
                            } else {
                                tracing::info!(
                                    latency_ms = %format!("{:.3}", result.latency_ms),
                                    drift_ms = %format!("{:.3}", drift),
                                    "Phase verified after ASIO reconnect, no toggle needed"
                                );
                            }
                        }
                    }

                    // Reset signal_lost if it was set
                    if signal_lost {
                        let lost_duration = signal_lost_since
                            .map(|t| t.elapsed().as_millis())
                            .unwrap_or(0);
                        signal_lost = false;
                        signal_lost_since = None;
                        if let Ok(mut store) = stats.lock() {
                            store.set_signal_lost(false);
                        }
                        tracing::info!(
                            latency_ms = %format!("{:.6}", result.latency_ms),
                            confidence = %format!("{:.3}", result.confidence),
                            lost_duration_ms = lost_duration,
                            "signal_recovered"
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
                        latency_ms = %format!("{:.6}", result.latency_ms),
                        confidence = %format!("{:.3}", result.confidence),
                        "signal_lost"
                    );
                }

                // Reset failure counter on successful analysis
                if consecutive_failures > 0 {
                    tracing::info!(
                        "Audio engine recovered after {} failed attempts",
                        consecutive_failures
                    );

                    // Record successful reconnection with actual duration
                    if reconnect_in_progress {
                        let duration = reconnect_start
                            .map(|s| s.elapsed().as_millis() as u64)
                            .unwrap_or(0);
                        if let Ok(mut store) = stats.lock() {
                            store.record_disconnection(duration, true);
                        }
                        reconnect_in_progress = false;
                        reconnect_start = None;
                    }
                }
                consecutive_failures = 0;

                // Record to stats store (preserve existing data - no clear!)
                if let Ok(mut store) = stats.lock() {
                    store.record_latency(result.latency_ms);
                    store.set_confidence(result.confidence);
                    tracing::debug!(
                        latency_ms = %format!("{:.6}", result.latency_ms),
                        confidence = %format!("{:.3}", result.confidence),
                        lost = result.lost_samples,
                        "stats_recorded"
                    );
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
                // Skip timeout check during probe grace period (engine just restarted)
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
                if !reconnect_in_progress {
                    reconnect_start = Some(std::time::Instant::now());
                }
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
                            // Prevent false signal loss after reconnect
                            last_successful_analysis = None;
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

                    // Record failed reconnection with actual duration
                    let duration = reconnect_start
                        .map(|s| s.elapsed().as_millis() as u64)
                        .unwrap_or(0);
                    if let Ok(mut store) = stats.lock() {
                        store.record_disconnection(duration, false);
                    }
                    reconnect_start = None;
                }
            }
        }

        // Signal-loss reconnection: when signal has been lost for >10s,
        // attempt to reconnect by stopping and restarting the engine.
        // This handles ASIO driver restarts (e.g. VBMatrix buffer changes)
        // where streams stay alive but receive silence.
        // Suppressed during ASIO restart recovery (which has its own settle/reconnect).
        if signal_lost && !reconnect_in_progress && !asio_restart_in_progress {
            if let Some(lost_since) = signal_lost_since {
                if lost_since.elapsed() > Duration::from_secs(10) {
                    tracing::warn!("Signal lost for >10s, attempting ASIO reconnection");

                    if last_status != tray::TrayStatus::Disconnected {
                        last_status = tray::TrayStatus::Disconnected;
                        emit_tray_status(tray::TrayStatus::Disconnected, 0.0, 0);
                    }

                    // Full reconnection: stop, re-select device, start
                    if let Err(e) = engine.stop().await {
                        tracing::debug!(error = %e, "Stop during signal-loss reconnect");
                    }

                    if let Some(ref device) = last_device_name {
                        if let Err(e) = engine.select_device(device.clone()).await {
                            tracing::warn!(error = %e, "Failed to re-select device");
                        }
                    }

                    match engine.start().await {
                        Ok(()) => {
                            tracing::info!("Engine restarted after signal loss");
                            last_successful_analysis = None;
                            signal_lost_since = Some(std::time::Instant::now());
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to restart after signal loss");
                            // Push the timer forward to retry in another 10s
                            signal_lost_since = Some(std::time::Instant::now());
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
            status,
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
