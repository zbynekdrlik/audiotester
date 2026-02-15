//! REST API endpoints for audiotester
//!
//! All endpoints are under /api/v1/ and return JSON.

use crate::AppState;
use audiotester_core::audio::engine::EngineState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde::{Deserialize, Serialize};

/// Application status response
#[derive(Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub build_date: String,
    pub state: String,
    pub device: Option<String>,
    pub sample_rate: u32,
    pub monitoring: bool,
}

/// Statistics response
#[derive(Serialize, Clone)]
pub struct StatsResponse {
    pub current_latency: f64,
    pub min_latency: f64,
    pub max_latency: f64,
    pub avg_latency: f64,
    pub total_lost: u64,
    pub total_corrupted: u64,
    pub measurement_count: u64,
    pub latency_history: Vec<(f64, f64)>,
    pub loss_history: Vec<(f64, f64)>,
    /// Active device name (if any)
    pub device_name: Option<String>,
    /// Current buffer size
    pub buffer_size: u32,
    /// Current sample rate
    pub sample_rate: u32,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Loss events with timestamps for visualization
    pub loss_events: Vec<LossEventResponse>,
    /// Total samples sent since reset
    pub samples_sent: u64,
    /// Total samples received since reset
    pub samples_received: u64,
    /// True when no signal is being received (analysis timeout)
    pub signal_lost: bool,
    /// Last correlation confidence (0.0 to 1.0, for debugging)
    pub confidence: f32,
}

/// Loss event response for API
#[derive(Serialize, Clone)]
pub struct LossEventResponse {
    /// Timestamp as ISO 8601 string
    pub timestamp: String,
    /// Number of samples lost
    pub count: u64,
}

/// Device info response
#[derive(Serialize)]
pub struct DeviceResponse {
    pub name: String,
    pub is_default: bool,
    pub sample_rates: Vec<u32>,
    pub input_channels: u16,
    pub output_channels: u16,
}

/// Configuration response
#[derive(Serialize, Deserialize)]
pub struct ConfigResponse {
    pub device: Option<String>,
    pub sample_rate: u32,
    pub monitoring: bool,
}

/// Configuration update request
#[derive(Deserialize)]
pub struct ConfigUpdate {
    pub device: Option<String>,
    pub sample_rate: Option<u32>,
}

/// Remote URL response
#[derive(Serialize)]
pub struct RemoteUrlResponse {
    pub url: String,
}

/// Monitoring toggle request
#[derive(Deserialize)]
pub struct MonitoringRequest {
    pub enabled: bool,
}

/// GET /api/v1/status
pub async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<StatusResponse>, (StatusCode, String)> {
    let status = state
        .engine
        .get_status()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(StatusResponse {
        version: audiotester_core::VERSION.to_string(),
        build_date: audiotester_core::BUILD_DATE.to_string(),
        state: format!("{:?}", status.state),
        device: status.device_name,
        sample_rate: status.sample_rate,
        monitoring: status.state == EngineState::Running,
    }))
}

/// GET /api/v1/stats
pub async fn get_stats(State(state): State<AppState>) -> Json<StatsResponse> {
    // Extract stats from lock in a block so MutexGuard is dropped before .await
    let (stats, latency_history, loss_history, loss_events) = {
        let store = state.stats.lock().unwrap();
        let stats = store.stats().clone();
        let latency_history = store.latency_plot_data(300);
        let loss_history = store.loss_plot_data(300);
        let loss_events: Vec<LossEventResponse> = store
            .loss_events()
            .iter()
            .rev()
            .take(100)
            .map(|e| LossEventResponse {
                timestamp: e.timestamp.to_rfc3339(),
                count: e.count,
            })
            .collect();
        (stats, latency_history, loss_history, loss_events)
    };

    // Get device info from engine (safe to await now, no lock held)
    let (device_name, sample_rate) = match state.engine.get_status().await {
        Ok(status) => (status.device_name, status.sample_rate),
        Err(_) => (None, 0),
    };

    Json(StatsResponse {
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
        device_name,
        buffer_size: stats.buffer_size,
        sample_rate,
        uptime_seconds: stats.uptime_seconds,
        loss_events,
        samples_sent: stats.samples_sent,
        samples_received: stats.samples_received,
        signal_lost: stats.signal_lost,
        confidence: stats.last_confidence,
    })
}

/// POST /api/v1/reset
///
/// Resets statistics counters (min/max/avg/totals) without clearing graph history.
pub async fn reset_stats(
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
    if let Ok(mut store) = state.stats.lock() {
        store.reset_counters();
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to acquire lock on stats store".to_string(),
        ))
    }
}

/// GET /api/v1/devices
pub async fn list_devices(
    State(state): State<AppState>,
) -> Result<Json<Vec<DeviceResponse>>, (StatusCode, String)> {
    match state.engine.list_devices().await {
        Ok(devices) => {
            let response: Vec<DeviceResponse> = devices
                .into_iter()
                .map(|d| DeviceResponse {
                    name: d.name,
                    is_default: d.is_default,
                    sample_rates: d.sample_rates,
                    input_channels: d.input_channels,
                    output_channels: d.output_channels,
                })
                .collect();
            Ok(Json(response))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list devices: {}", e),
        )),
    }
}

/// GET /api/v1/config
pub async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<ConfigResponse>, (StatusCode, String)> {
    let status = state
        .engine
        .get_status()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ConfigResponse {
        device: status.device_name,
        sample_rate: status.sample_rate,
        monitoring: status.state == EngineState::Running,
    }))
}

/// PATCH /api/v1/config
pub async fn update_config(
    State(state): State<AppState>,
    Json(update): Json<ConfigUpdate>,
) -> Result<Json<ConfigResponse>, (StatusCode, String)> {
    if let Some(rate) = update.sample_rate {
        if !(8000..=384000).contains(&rate) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid sample rate: {} (must be 8000-384000 Hz)", rate),
            ));
        }
        state.engine.set_sample_rate(rate).await;
    }

    if let Some(ref device) = update.device {
        // Stop if running
        let status = state
            .engine
            .get_status()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if status.state == EngineState::Running {
            state.engine.stop().await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to stop: {}", e),
                )
            })?;
        }

        state
            .engine
            .select_device(device.clone())
            .await
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Failed to select device: {}", e),
                )
            })?;
    }

    let status = state
        .engine
        .get_status()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ConfigResponse {
        device: status.device_name,
        sample_rate: status.sample_rate,
        monitoring: status.state == EngineState::Running,
    }))
}

/// GET /api/v1/remote-url
///
/// Returns the remote access URL for accessing the dashboard from other devices.
pub async fn get_remote_url(State(state): State<AppState>) -> Json<RemoteUrlResponse> {
    let ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".to_string());
    Json(RemoteUrlResponse {
        url: format!("http://{}:{}", ip, state.config.port),
    })
}

/// POST /api/v1/monitoring
pub async fn toggle_monitoring(
    State(state): State<AppState>,
    Json(req): Json<MonitoringRequest>,
) -> Result<Json<StatusResponse>, (StatusCode, String)> {
    let current = state
        .engine
        .get_status()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if req.enabled {
        if current.state != EngineState::Running {
            // Allow ASIO driver time to release resources after stop().
            // VBMatrix VASIO-8 can hold exclusive device access for several
            // seconds after streams are dropped.
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

            // Retry loop with exponential backoff: ASIO drivers (especially
            // VBMatrix VASIO-8) may need up to ~10 seconds to fully release
            // resources after stop/start cycles.
            let max_attempts = 5u32;
            let mut last_error = String::new();
            let mut started = false;

            for attempt in 1..=max_attempts {
                // Re-select device to get a fresh ASIO handle before starting.
                // After reboot or driver restart, the stored handle may be stale.
                if let Some(ref device) = current.device_name {
                    if let Err(e) = state.engine.select_device(device.clone()).await {
                        last_error =
                            format!("Failed to re-select device (attempt {}): {}", attempt, e);
                        tracing::warn!("{}", last_error);
                        if attempt < max_attempts {
                            // Exponential backoff: 1s, 2s, 4s, 8s
                            let delay = 1000u64 * 2u64.pow(attempt - 1);
                            let delay = delay.min(8000); // cap at 8s
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                        continue;
                    }
                }

                match state.engine.start().await {
                    Ok(()) => {
                        if attempt > 1 {
                            tracing::info!("Monitoring started on attempt {}", attempt);
                        }
                        started = true;
                        break;
                    }
                    Err(e) => {
                        last_error = format!("Failed to start (attempt {}): {}", attempt, e);
                        tracing::warn!("{}", last_error);
                        if attempt < max_attempts {
                            // Exponential backoff: 1s, 2s, 4s, 8s
                            let delay = 1000u64 * 2u64.pow(attempt - 1);
                            let delay = delay.min(8000); // cap at 8s
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                    }
                }
            }

            if !started {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, last_error));
            }
        }
    } else if current.state == EngineState::Running {
        state.engine.stop().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to stop: {}", e),
            )
        })?;
    }

    let status = state
        .engine
        .get_status()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(StatusResponse {
        version: audiotester_core::VERSION.to_string(),
        build_date: audiotester_core::BUILD_DATE.to_string(),
        state: format!("{:?}", status.state),
        device: status.device_name,
        sample_rate: status.sample_rate,
        monitoring: status.state == EngineState::Running,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_response_serializes() {
        let resp = StatusResponse {
            version: "0.1.5".to_string(),
            build_date: "2026-02-15".to_string(),
            state: "Stopped".to_string(),
            device: None,
            sample_rate: 96000,
            monitoring: false,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"version\":\"0.1.5\""));
        assert!(json.contains("\"build_date\":\"2026-02-15\""));
    }

    #[test]
    fn test_stats_response_serializes() {
        let resp = StatsResponse {
            current_latency: 5.0,
            min_latency: 4.0,
            max_latency: 6.0,
            avg_latency: 5.0,
            total_lost: 0,
            total_corrupted: 0,
            measurement_count: 100,
            latency_history: vec![(-1.0, 5.0), (-2.0, 5.1)],
            loss_history: vec![],
            device_name: Some("Test ASIO".to_string()),
            buffer_size: 256,
            sample_rate: 96000,
            uptime_seconds: 3600,
            loss_events: vec![],
            samples_sent: 1000000,
            samples_received: 999950,
            signal_lost: false,
            confidence: 0.85,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"current_latency\":5.0"));
        assert!(json.contains("\"device_name\":\"Test ASIO\""));
        assert!(json.contains("\"sample_rate\":96000"));
        assert!(json.contains("\"samples_sent\":1000000"));
        assert!(json.contains("\"samples_received\":999950"));
        assert!(json.contains("\"signal_lost\":false"));
        assert!(json.contains("\"confidence\":0.85"));
    }

    #[test]
    fn test_config_update_deserializes() {
        let json = r#"{"device": "Test Device", "sample_rate": 48000}"#;
        let update: ConfigUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(update.device, Some("Test Device".to_string()));
        assert_eq!(update.sample_rate, Some(48000));
    }

    #[test]
    fn test_config_update_partial() {
        let json = r#"{"sample_rate": 48000}"#;
        let update: ConfigUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(update.device, None);
        assert_eq!(update.sample_rate, Some(48000));
    }
}
