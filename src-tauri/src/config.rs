//! Persistent application configuration
//!
//! Stores device selection, sample rate, and channel pair in a JSON file
//! at `%APPDATA%/audiotester/config.json` (Windows) or equivalent.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_sample_rate() -> u32 {
    audiotester_core::DEFAULT_SAMPLE_RATE
}

fn default_channel_pair() -> [u16; 2] {
    [1, 2]
}

/// Persistent application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Selected ASIO device name (None = no device remembered)
    #[serde(default)]
    pub device: Option<String>,
    /// Sample rate in Hz
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    /// Channel pair [signal, counter] as 1-based indices
    #[serde(default = "default_channel_pair")]
    pub channel_pair: [u16; 2],
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device: None,
            sample_rate: default_sample_rate(),
            channel_pair: default_channel_pair(),
        }
    }
}

impl AppConfig {
    /// Config file path: `<data_dir>/audiotester/config.json`
    pub fn path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("audiotester")
            .join("config.json")
    }

    /// Load config from disk, falling back to defaults on any error
    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(config) => {
                    tracing::info!(path = %path.display(), "Loaded config from disk");
                    config
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to parse config, using defaults");
                    Self::default()
                }
            },
            Err(_) => {
                tracing::info!(path = %path.display(), "No config file found, using defaults");
                Self::default()
            }
        }
    }

    /// Save config to disk, creating parent directories if needed
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        tracing::info!(path = %path.display(), "Config saved to disk");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.device, None);
        assert_eq!(config.sample_rate, 96000);
        assert_eq!(config.channel_pair, [1, 2]);
    }

    #[test]
    fn test_round_trip() {
        let config = AppConfig {
            device: Some("ASIO128".to_string()),
            sample_rate: 48000,
            channel_pair: [127, 128],
        };
        let json = serde_json::to_string(&config).unwrap();
        let loaded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.device, Some("ASIO128".to_string()));
        assert_eq!(loaded.sample_rate, 48000);
        assert_eq!(loaded.channel_pair, [127, 128]);
    }

    #[test]
    fn test_missing_fields_use_defaults() {
        let json = r#"{"device": "TestDevice"}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.device, Some("TestDevice".to_string()));
        assert_eq!(config.sample_rate, 96000);
        assert_eq!(config.channel_pair, [1, 2]);
    }

    #[test]
    fn test_empty_json_uses_defaults() {
        let json = "{}";
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.device, None);
        assert_eq!(config.sample_rate, 96000);
        assert_eq!(config.channel_pair, [1, 2]);
    }

    #[test]
    fn test_save_and_load() {
        let dir = std::env::temp_dir().join("audiotester-config-test");
        let path = dir.join("config.json");
        let _ = std::fs::remove_dir_all(&dir);

        let config = AppConfig {
            device: Some("Test ASIO".to_string()),
            sample_rate: 96000,
            channel_pair: [3, 4],
        };
        config.save(&path).unwrap();

        let loaded: AppConfig =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.device, Some("Test ASIO".to_string()));
        assert_eq!(loaded.channel_pair, [3, 4]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
