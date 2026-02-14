//! Audiotester Core - Audio engine, signal processing, and statistics
//!
//! This library provides the core functionality for monitoring professional audio paths
//! including Dante, VBAN, and VBMatrix connections. It measures latency using
//! timestamp-based burst detection and detects sample loss via frame counter tracking.
//!
//! ## Architecture
//!
//! The latency measurement system uses a two-channel approach:
//! - **Channel 0**: Burst signal (10ms noise every 100ms) for latency measurement
//! - **Channel 1**: Frame counter (sawtooth 0.0-1.0) for loss detection
//!
//! Latency is calculated by comparing burst generation timestamps with detection
//! timestamps, providing sub-millisecond accuracy without the buffer accumulation
//! delays of correlation-based methods.

pub mod audio;
pub mod stats;

// Primary exports - new burst-based latency system
pub use audio::burst::{BurstEvent, BurstGenerator};
pub use audio::detector::BurstDetector;
pub use audio::engine::{AudioEngine, ConnectionState};
pub use audio::latency::{LatencyAnalyzer, LatencyResult};

// Frame-based loss detection
pub use audio::analyzer::Analyzer;

// Legacy MLS exports (for backward compatibility and fallback)
pub use audio::signal::MlsGenerator;

pub use stats::store::{DisconnectionEvent, LossEvent, StatsStore};

/// Application version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default sample rate for audio processing (96kHz for professional setups)
pub const DEFAULT_SAMPLE_RATE: u32 = 96000;

/// Burst cycle duration in milliseconds (100ms = 10Hz update rate)
pub const BURST_CYCLE_MS: u32 = 100;

/// Burst duration in milliseconds (10ms of noise per cycle)
pub const BURST_DURATION_MS: u32 = 10;

/// MLS sequence order (2^ORDER - 1 samples) - legacy, for fallback
pub const MLS_ORDER: u32 = 15;

/// MLS sequence length (32767 samples at order 15) - legacy, for fallback
pub const MLS_LENGTH: usize = (1 << MLS_ORDER) - 1;
