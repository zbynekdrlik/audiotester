//! Audiotester Web Server - Axum + Leptos SSR
//!
//! Provides a web interface for monitoring audio statistics,
//! accessible from both local desktop and remote browsers.

pub mod api;
pub mod ui;
pub mod ws;

use audiotester_core::audio::engine::{AnalysisResult, AudioEngine, DeviceInfo, EngineState};
use audiotester_core::stats::store::StatsStore;
use axum::http::{header, HeaderValue};
use axum::response::IntoResponse;
use axum::Router;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

/// Commands sent to the engine thread
pub enum EngineCommand {
    ListDevices {
        reply: oneshot::Sender<anyhow::Result<Vec<DeviceInfo>>>,
    },
    SelectDevice {
        name: String,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    SetSampleRate {
        rate: u32,
    },
    Start {
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    Stop {
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    GetStatus {
        reply: oneshot::Sender<EngineStatus>,
    },
    Analyze {
        reply: oneshot::Sender<Option<AnalysisResult>>,
    },
    GetSampleCounts {
        reply: oneshot::Sender<(usize, usize)>,
    },
    IsStreamInvalidated {
        reply: oneshot::Sender<bool>,
    },
}

/// Engine status snapshot (safe to send between threads)
#[derive(Clone, Debug)]
pub struct EngineStatus {
    pub state: EngineState,
    pub device_name: Option<String>,
    pub sample_rate: u32,
}

/// Handle to communicate with the engine thread
#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<EngineCommand>,
}

impl EngineHandle {
    /// Spawn the engine on a dedicated thread and return a handle
    pub fn spawn() -> Self {
        let (tx, mut rx) = mpsc::channel::<EngineCommand>(32);

        std::thread::spawn(move || {
            let mut engine = AudioEngine::new();

            while let Some(cmd) = rx.blocking_recv() {
                match cmd {
                    EngineCommand::ListDevices { reply } => {
                        let _ = reply.send(AudioEngine::list_devices());
                    }
                    EngineCommand::SelectDevice { name, reply } => {
                        let _ = reply.send(engine.select_device(&name));
                    }
                    EngineCommand::SetSampleRate { rate } => {
                        engine.set_sample_rate(rate);
                    }
                    EngineCommand::Start { reply } => {
                        let _ = reply.send(engine.start());
                    }
                    EngineCommand::Stop { reply } => {
                        let _ = reply.send(engine.stop());
                    }
                    EngineCommand::GetStatus { reply } => {
                        let _ = reply.send(EngineStatus {
                            state: engine.state(),
                            device_name: engine.device_name().map(|s| s.to_string()),
                            sample_rate: engine.sample_rate(),
                        });
                    }
                    EngineCommand::Analyze { reply } => {
                        let _ = reply.send(engine.analyze());
                    }
                    EngineCommand::GetSampleCounts { reply } => {
                        let _ = reply.send(engine.sample_counts());
                    }
                    EngineCommand::IsStreamInvalidated { reply } => {
                        let _ = reply.send(engine.is_stream_invalidated());
                    }
                }
            }
        });

        Self { tx }
    }

    pub async fn list_devices(&self) -> anyhow::Result<Vec<DeviceInfo>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EngineCommand::ListDevices { reply })
            .await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?
    }

    pub async fn select_device(&self, name: String) -> anyhow::Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EngineCommand::SelectDevice { name, reply })
            .await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?
    }

    pub async fn set_sample_rate(&self, rate: u32) {
        let _ = self.tx.send(EngineCommand::SetSampleRate { rate }).await;
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EngineCommand::Start { reply })
            .await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EngineCommand::Stop { reply })
            .await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?
    }

    pub async fn get_status(&self) -> anyhow::Result<EngineStatus> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EngineCommand::GetStatus { reply })
            .await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Engine thread died"))
    }

    pub async fn analyze(&self) -> anyhow::Result<Option<AnalysisResult>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EngineCommand::Analyze { reply })
            .await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Engine thread died"))
    }

    /// Get sample counts from the audio engine
    ///
    /// Returns (output_samples, input_samples) as cumulative counters
    pub async fn get_sample_counts(&self) -> anyhow::Result<(usize, usize)> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EngineCommand::GetSampleCounts { reply })
            .await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Engine thread died"))
    }

    /// Check if the ASIO driver sent a stream invalidation (kAsioResetRequest).
    ///
    /// Returns true when the driver has reset and streams need to be rebuilt.
    pub async fn is_stream_invalidated(&self) -> anyhow::Result<bool> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EngineCommand::IsStreamInvalidated { reply })
            .await
            .map_err(|_| anyhow::anyhow!("Engine thread died"))?;
        rx.await.map_err(|_| anyhow::anyhow!("Engine thread died"))
    }
}

/// Shared application state accessible from all handlers
#[derive(Clone)]
pub struct AppState {
    /// Handle to engine thread
    pub engine: EngineHandle,
    /// Statistics store for recording measurements
    pub stats: Arc<Mutex<StatsStore>>,
    /// WebSocket broadcast channel
    pub ws_tx: tokio::sync::broadcast::Sender<String>,
    /// Server configuration
    pub config: ServerConfig,
    /// Log directory for diagnostic file logging
    pub log_dir: Option<std::path::PathBuf>,
}

/// Server configuration
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Port to listen on
    pub port: u16,
    /// Bind address
    pub bind_addr: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8920,
            bind_addr: "0.0.0.0".to_string(),
        }
    }
}

impl AppState {
    /// Create a new AppState with the given engine handle and stats store
    pub fn new(
        engine: EngineHandle,
        stats: Arc<Mutex<StatsStore>>,
        config: ServerConfig,
        log_dir: Option<std::path::PathBuf>,
    ) -> Self {
        let (ws_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            engine,
            stats,
            ws_tx,
            config,
            log_dir,
        }
    }
}

/// Serve the PWA manifest.json
async fn serve_manifest() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        ui::MANIFEST_JSON,
    )
}

/// Build the Axum router with all routes
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Leptos SSR pages
        .route("/", axum::routing::get(ui::dashboard::dashboard_page))
        .route("/settings", axum::routing::get(ui::settings::settings_page))
        // REST API
        .route("/api/v1/status", axum::routing::get(api::get_status))
        .route("/api/v1/stats", axum::routing::get(api::get_stats))
        .route("/api/v1/devices", axum::routing::get(api::list_devices))
        .route(
            "/api/v1/config",
            axum::routing::get(api::get_config).patch(api::update_config),
        )
        .route(
            "/api/v1/monitoring",
            axum::routing::post(api::toggle_monitoring),
        )
        .route("/api/v1/reset", axum::routing::post(api::reset_stats))
        .route(
            "/api/v1/remote-url",
            axum::routing::get(api::get_remote_url),
        )
        // Diagnostic logs
        .route("/api/v1/logs", axum::routing::get(api::get_logs))
        // WebSocket
        .route("/api/v1/ws", axum::routing::get(ws::ws_handler))
        // PWA manifest
        .route("/manifest.json", axum::routing::get(serve_manifest))
        // Static assets (CSS, JS)
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(CorsLayer::permissive())
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .with_state(state)
}

/// Start the web server
pub async fn start_server(state: AppState) -> anyhow::Result<()> {
    let addr = format!("{}:{}", state.config.bind_addr, state.config.port);
    let app = build_router(state);

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "Audiotester web server listening");

    axum::serve(listener, app).await?;
    Ok(())
}
