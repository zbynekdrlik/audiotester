//! Audiotester - Windows ASIO audio testing application
//!
//! This library re-exports the core audio engine, signal processing,
//! and statistics functionality from `audiotester-core`.
//!
//! ## Architecture
//!
//! The latency measurement system uses timestamp-based burst detection:
//! - Channel 0: 10ms burst every 100ms (10Hz update rate)
//! - Channel 1: Frame counter (sawtooth 0.0-1.0) for loss detection
//!
//! The UI has migrated to Tauri 2 + Leptos SSR (see `audiotester-server` and `src-tauri/`).

pub use audiotester_core::audio;
pub use audiotester_core::stats;

// Primary exports - burst-based latency system
pub use audiotester_core::AudioEngine;
pub use audiotester_core::{BurstDetector, BurstEvent, BurstGenerator};
pub use audiotester_core::{LatencyAnalyzer, LatencyResult};

// Frame-based loss detection
pub use audiotester_core::Analyzer;

// Legacy MLS exports (for backward compatibility)
pub use audiotester_core::MlsGenerator;

// Statistics
pub use audiotester_core::StatsStore;

// Constants
pub use audiotester_core::{BURST_CYCLE_MS, BURST_DURATION_MS};
pub use audiotester_core::{DEFAULT_SAMPLE_RATE, MLS_LENGTH, MLS_ORDER, VERSION};
