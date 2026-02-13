//! Audiotester Core - Audio engine, signal processing, and statistics
//!
//! This library provides the core functionality for monitoring professional audio paths
//! including Dante, VBAN, and VBMatrix connections. It measures latency and
//! detects sample loss using Maximum Length Sequence (MLS) test signals.

pub mod audio;
pub mod stats;

pub use audio::{analyzer::Analyzer, engine::AudioEngine, signal::MlsGenerator};
pub use stats::store::StatsStore;

/// Application version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default sample rate for audio processing (96kHz for professional setups)
pub const DEFAULT_SAMPLE_RATE: u32 = 96000;

/// MLS sequence order (2^ORDER - 1 samples)
pub const MLS_ORDER: u32 = 15;

/// MLS sequence length (32767 samples at order 15)
pub const MLS_LENGTH: usize = (1 << MLS_ORDER) - 1;
