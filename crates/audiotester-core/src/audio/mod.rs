//! Audio processing module
//!
//! This module contains all audio-related functionality including:
//! - ASIO device management ([`engine`])
//! - Burst signal generation for latency measurement ([`burst`])
//! - Envelope-based burst detection ([`detector`])
//! - Timestamp-based latency calculation ([`latency`])
//! - Frame counter analysis for loss detection ([`analyzer`])
//! - MLS test signal generation (legacy, [`signal`])

pub mod analyzer;
pub mod burst;
pub mod detector;
pub mod engine;
pub mod latency;
pub mod signal;
