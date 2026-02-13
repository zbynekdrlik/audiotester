//! Audiotester - Windows ASIO audio testing application
//!
//! This library re-exports the core audio engine, signal processing,
//! and statistics functionality from `audiotester-core`.
//!
//! The UI has migrated to Tauri 2 + Leptos SSR (see `audiotester-server` and `src-tauri/`).

pub use audiotester_core::audio;
pub use audiotester_core::stats;

pub use audiotester_core::{Analyzer, AudioEngine, MlsGenerator, StatsStore};
pub use audiotester_core::{DEFAULT_SAMPLE_RATE, MLS_LENGTH, MLS_ORDER, VERSION};
