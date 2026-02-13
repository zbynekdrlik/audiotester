//! Time-series data storage for statistics
//!
//! Stores historical measurements with automatic cleanup of old data.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// Maximum number of data points to keep
const MAX_HISTORY_SIZE: usize = 3600; // 1 hour at 1 sample/sec

/// A single measurement point
#[derive(Debug, Clone)]
pub struct Measurement {
    /// Timestamp of the measurement
    pub timestamp: DateTime<Utc>,
    /// Value of the measurement
    pub value: f64,
}

/// Statistics store for time-series data
#[derive(Debug)]
pub struct StatsStore {
    /// Latency measurements (ms)
    latency_history: VecDeque<Measurement>,
    /// Sample loss count over time
    loss_history: VecDeque<Measurement>,
    /// Corruption events over time
    corruption_history: VecDeque<Measurement>,
    /// Maximum history size
    max_size: usize,
    /// Running statistics
    stats: RunningStats,
}

/// Running statistics calculated from measurements
#[derive(Debug, Default, Clone)]
pub struct RunningStats {
    /// Current latency (ms)
    pub current_latency: f64,
    /// Minimum latency observed (ms)
    pub min_latency: f64,
    /// Maximum latency observed (ms)
    pub max_latency: f64,
    /// Average latency (ms)
    pub avg_latency: f64,
    /// Total samples lost
    pub total_lost: u64,
    /// Total samples corrupted
    pub total_corrupted: u64,
    /// Measurement count
    pub measurement_count: u64,
}

impl StatsStore {
    /// Create a new statistics store
    pub fn new() -> Self {
        Self {
            latency_history: VecDeque::with_capacity(MAX_HISTORY_SIZE),
            loss_history: VecDeque::with_capacity(MAX_HISTORY_SIZE),
            corruption_history: VecDeque::with_capacity(MAX_HISTORY_SIZE),
            max_size: MAX_HISTORY_SIZE,
            stats: RunningStats {
                min_latency: f64::MAX,
                ..Default::default()
            },
        }
    }

    /// Record a latency measurement
    ///
    /// # Arguments
    /// * `latency_ms` - Latency in milliseconds
    pub fn record_latency(&mut self, latency_ms: f64) {
        let measurement = Measurement {
            timestamp: Utc::now(),
            value: latency_ms,
        };

        // Update history
        if self.latency_history.len() >= self.max_size {
            self.latency_history.pop_front();
        }
        self.latency_history.push_back(measurement);

        // Update running stats
        self.stats.current_latency = latency_ms;
        self.stats.min_latency = self.stats.min_latency.min(latency_ms);
        self.stats.max_latency = self.stats.max_latency.max(latency_ms);
        self.stats.measurement_count += 1;

        // Recalculate average
        let sum: f64 = self.latency_history.iter().map(|m| m.value).sum();
        self.stats.avg_latency = sum / self.latency_history.len() as f64;
    }

    /// Record sample loss
    ///
    /// # Arguments
    /// * `count` - Number of samples lost
    pub fn record_loss(&mut self, count: u64) {
        let measurement = Measurement {
            timestamp: Utc::now(),
            value: count as f64,
        };

        if self.loss_history.len() >= self.max_size {
            self.loss_history.pop_front();
        }
        self.loss_history.push_back(measurement);

        self.stats.total_lost += count;
    }

    /// Record sample corruption
    ///
    /// # Arguments
    /// * `count` - Number of corrupted samples
    pub fn record_corruption(&mut self, count: u64) {
        let measurement = Measurement {
            timestamp: Utc::now(),
            value: count as f64,
        };

        if self.corruption_history.len() >= self.max_size {
            self.corruption_history.pop_front();
        }
        self.corruption_history.push_back(measurement);

        self.stats.total_corrupted += count;
    }

    /// Get latency history
    pub fn latency_history(&self) -> &VecDeque<Measurement> {
        &self.latency_history
    }

    /// Get loss history
    pub fn loss_history(&self) -> &VecDeque<Measurement> {
        &self.loss_history
    }

    /// Get corruption history
    pub fn corruption_history(&self) -> &VecDeque<Measurement> {
        &self.corruption_history
    }

    /// Get running statistics
    pub fn stats(&self) -> &RunningStats {
        &self.stats
    }

    /// Clear all history and reset statistics
    pub fn clear(&mut self) {
        self.latency_history.clear();
        self.loss_history.clear();
        self.corruption_history.clear();
        self.stats = RunningStats {
            min_latency: f64::MAX,
            ..Default::default()
        };
    }

    /// Get latency values for plotting (last N points)
    ///
    /// # Returns
    /// Vector of (time_offset_seconds, latency_ms) pairs
    pub fn latency_plot_data(&self, count: usize) -> Vec<(f64, f64)> {
        let now = Utc::now();
        self.latency_history
            .iter()
            .rev()
            .take(count)
            .map(|m| {
                let time_offset = (now - m.timestamp).num_milliseconds() as f64 / 1000.0;
                (-time_offset, m.value)
            })
            .collect()
    }

    /// Get loss values for plotting (last N points)
    ///
    /// # Returns
    /// Vector of (time_offset_seconds, loss_count) pairs
    pub fn loss_plot_data(&self, count: usize) -> Vec<(f64, f64)> {
        let now = Utc::now();
        self.loss_history
            .iter()
            .rev()
            .take(count)
            .map(|m| {
                let time_offset = (now - m.timestamp).num_milliseconds() as f64 / 1000.0;
                (-time_offset, m.value)
            })
            .collect()
    }
}

impl Default for StatsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_creation() {
        let store = StatsStore::new();
        assert_eq!(store.latency_history().len(), 0);
        assert_eq!(store.stats().measurement_count, 0);
    }

    #[test]
    fn test_record_latency() {
        let mut store = StatsStore::new();

        store.record_latency(5.0);
        assert_eq!(store.stats().current_latency, 5.0);
        assert_eq!(store.stats().measurement_count, 1);

        store.record_latency(10.0);
        assert_eq!(store.stats().current_latency, 10.0);
        assert_eq!(store.stats().min_latency, 5.0);
        assert_eq!(store.stats().max_latency, 10.0);
        assert_eq!(store.stats().avg_latency, 7.5);
    }

    #[test]
    fn test_record_loss() {
        let mut store = StatsStore::new();

        store.record_loss(10);
        assert_eq!(store.stats().total_lost, 10);

        store.record_loss(5);
        assert_eq!(store.stats().total_lost, 15);
    }

    #[test]
    fn test_clear() {
        let mut store = StatsStore::new();

        store.record_latency(5.0);
        store.record_loss(10);
        store.clear();

        assert_eq!(store.latency_history().len(), 0);
        assert_eq!(store.stats().total_lost, 0);
    }

    #[test]
    fn test_history_limit() {
        let mut store = StatsStore::new();

        // Fill beyond capacity
        for i in 0..4000 {
            store.record_latency(i as f64);
        }

        // Should be limited to MAX_HISTORY_SIZE
        assert_eq!(store.latency_history().len(), MAX_HISTORY_SIZE);
    }
}
