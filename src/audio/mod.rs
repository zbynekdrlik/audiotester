//! Audio processing module
//!
//! This module contains all audio-related functionality including:
//! - ASIO device management ([`engine`])
//! - MLS test signal generation ([`signal`])
//! - Cross-correlation and analysis ([`analyzer`])

pub mod analyzer;
pub mod engine;
pub mod signal;
