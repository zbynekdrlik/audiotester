//! Time-series data storage for statistics
//!
//! Stores historical measurements with automatic cleanup of old data.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// Maximum number of data points to keep in recent history (full resolution)
const MAX_HISTORY_SIZE: usize = 3600; // 1 hour at 1 sample/sec

/// Maximum number of data points in archive (down-sampled)
const MAX_ARCHIVE_SIZE: usize = 8640; // 24 hours at 10-second intervals

/// Duration of each loss archive bucket in seconds
const LOSS_BUCKET_DURATION_SECS: i64 = 10;

/// Maximum number of loss archive buckets (24h at 10s = 8640)
const MAX_LOSS_ARCHIVE_SIZE: usize = 8640;

/// A single measurement point
#[derive(Debug, Clone)]
pub struct Measurement {
    /// Timestamp of the measurement
    pub timestamp: DateTime<Utc>,
    /// Value of the measurement
    pub value: f64,
}

/// A disconnection event with timestamp and duration
#[derive(Debug, Clone)]
pub struct DisconnectionEvent {
    /// When the disconnection was detected
    pub timestamp: DateTime<Utc>,
    /// Duration of the disconnection in milliseconds (0 if still disconnected)
    pub duration_ms: u64,
    /// Whether reconnection was successful
    pub reconnected: bool,
}

/// A loss event with timestamp and count
#[derive(Debug, Clone)]
pub struct LossEvent {
    /// When the loss was detected
    pub timestamp: DateTime<Utc>,
    /// Number of samples lost
    pub count: u64,
}

/// Aggregated loss over a fixed time window (10 seconds)
#[derive(Debug, Clone)]
pub struct LossBucket {
    /// Start of this bucket (truncated to LOSS_BUCKET_DURATION_SECS boundary)
    pub timestamp: DateTime<Utc>,
    /// Total samples lost in this bucket
    pub total_loss: u64,
    /// Number of discrete loss events in this bucket
    pub event_count: u32,
}

/// Statistics store for time-series data
#[derive(Debug)]
pub struct StatsStore {
    /// Latency measurements (ms) - recent full resolution
    latency_history: VecDeque<Measurement>,
    /// Latency archive - down-sampled for extended history
    latency_archive: VecDeque<Measurement>,
    /// Sample loss count over time
    loss_history: VecDeque<Measurement>,
    /// Corruption events over time
    corruption_history: VecDeque<Measurement>,
    /// Disconnection events
    disconnection_events: Vec<DisconnectionEvent>,
    /// Loss events with timestamps
    loss_events: Vec<LossEvent>,
    /// Loss archive: 10-second buckets for 24h timeline
    loss_archive: VecDeque<LossBucket>,
    /// Maximum history size
    max_size: usize,
    /// Maximum archive size
    max_archive_size: usize,
    /// Running statistics
    stats: RunningStats,
    /// Counter for archive down-sampling (archive every N measurements)
    archive_counter: u64,
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
    /// Uptime in seconds since monitoring started
    pub uptime_seconds: u64,
    /// Connected device name (cached from engine)
    pub device_name: Option<String>,
    /// Current sample rate (cached from engine)
    pub sample_rate: u32,
    /// Current buffer size (cached from engine)
    pub buffer_size: u32,
    /// Total samples sent since reset
    pub samples_sent: u64,
    /// Total samples received since reset
    pub samples_received: u64,
    /// True when no signal is being received (analysis timeout)
    pub signal_lost: bool,
    /// Last correlation confidence (0.0 to 1.0)
    pub last_confidence: f32,
    /// Estimated missing samples while counter signal was absent
    pub estimated_loss: u64,
    /// True when ch1 counter signal is currently absent (muted loopback)
    pub counter_silent: bool,
}

impl StatsStore {
    /// Create a new statistics store
    pub fn new() -> Self {
        Self {
            latency_history: VecDeque::with_capacity(MAX_HISTORY_SIZE),
            latency_archive: VecDeque::with_capacity(MAX_ARCHIVE_SIZE),
            loss_history: VecDeque::with_capacity(MAX_HISTORY_SIZE),
            corruption_history: VecDeque::with_capacity(MAX_HISTORY_SIZE),
            disconnection_events: Vec::new(),
            loss_events: Vec::new(),
            loss_archive: VecDeque::with_capacity(MAX_LOSS_ARCHIVE_SIZE),
            max_size: MAX_HISTORY_SIZE,
            max_archive_size: MAX_ARCHIVE_SIZE,
            stats: RunningStats {
                min_latency: f64::MAX,
                ..Default::default()
            },
            archive_counter: 0,
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
        self.latency_history.push_back(measurement.clone());

        // Archive down-sampled data (every 10 measurements)
        self.archive_counter += 1;
        if self.archive_counter.is_multiple_of(10) {
            if self.latency_archive.len() >= self.max_archive_size {
                self.latency_archive.pop_front();
            }
            self.latency_archive.push_back(measurement);
        }

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
        let now = Utc::now();
        let measurement = Measurement {
            timestamp: now,
            value: count as f64,
        };

        if self.loss_history.len() >= self.max_size {
            self.loss_history.pop_front();
        }
        self.loss_history.push_back(measurement);

        // Record as a loss event
        self.loss_events.push(LossEvent {
            timestamp: now,
            count,
        });

        // Aggregate into loss_archive bucket
        let bucket_ts = Self::truncate_to_bucket(now);
        if let Some(last) = self.loss_archive.back_mut() {
            if last.timestamp == bucket_ts {
                // Same bucket — aggregate
                last.total_loss += count;
                last.event_count += 1;
            } else {
                // New bucket
                if self.loss_archive.len() >= MAX_LOSS_ARCHIVE_SIZE {
                    self.loss_archive.pop_front();
                }
                self.loss_archive.push_back(LossBucket {
                    timestamp: bucket_ts,
                    total_loss: count,
                    event_count: 1,
                });
            }
        } else {
            // First bucket ever
            self.loss_archive.push_back(LossBucket {
                timestamp: bucket_ts,
                total_loss: count,
                event_count: 1,
            });
        }

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
        self.latency_archive.clear();
        self.loss_history.clear();
        self.corruption_history.clear();
        self.disconnection_events.clear();
        self.loss_events.clear();
        self.loss_archive.clear();
        self.archive_counter = 0;
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

    /// Reset counters without clearing history
    ///
    /// Resets min/max/avg latency and loss/corruption totals,
    /// but preserves the graph history data for continued visualization.
    pub fn reset_counters(&mut self) {
        self.stats.min_latency = f64::MAX;
        self.stats.max_latency = 0.0;
        self.stats.avg_latency = 0.0;
        self.stats.total_lost = 0;
        self.stats.total_corrupted = 0;
        self.stats.measurement_count = 0;
        self.stats.uptime_seconds = 0;
        self.stats.samples_sent = 0;
        self.stats.samples_received = 0;
        self.stats.estimated_loss = 0;
        self.stats.counter_silent = false;
    }

    /// Truncate a timestamp to the nearest LOSS_BUCKET_DURATION_SECS boundary
    fn truncate_to_bucket(ts: DateTime<Utc>) -> DateTime<Utc> {
        let secs = ts.timestamp();
        let truncated = secs - (secs % LOSS_BUCKET_DURATION_SECS);
        DateTime::from_timestamp(truncated, 0).unwrap_or(ts)
    }

    /// Called every 10 seconds from the monitoring loop.
    ///
    /// Ensures continuous timeline coverage by appending a zero-loss bucket
    /// when the most recent bucket is older than LOSS_BUCKET_DURATION_SECS.
    /// This lets the chart display the full monitored timespan with empty gaps.
    pub fn loss_archive_tick(&mut self) {
        let now = Utc::now();
        let bucket_ts = Self::truncate_to_bucket(now);

        let should_push = match self.loss_archive.back() {
            Some(last) => last.timestamp < bucket_ts,
            None => false, // Don't push zero buckets if archive is empty (no monitoring data)
        };

        if should_push {
            if self.loss_archive.len() >= MAX_LOSS_ARCHIVE_SIZE {
                self.loss_archive.pop_front();
            }
            self.loss_archive.push_back(LossBucket {
                timestamp: bucket_ts,
                total_loss: 0,
                event_count: 0,
            });
        }
    }

    /// Query loss timeline data for a given time range, re-aggregated into larger buckets.
    ///
    /// # Arguments
    /// * `range_secs` - How far back to look (e.g. 3600 for 1h, 86400 for 24h)
    /// * `bucket_size_secs` - Desired output bucket size in seconds (must be >= 10)
    ///
    /// # Returns
    /// Vector of (unix_timestamp, total_loss, event_count) tuples, sorted by time
    pub fn loss_timeline_data(
        &self,
        range_secs: i64,
        bucket_size_secs: i64,
    ) -> Vec<(i64, u64, u32)> {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(range_secs);
        let bucket_size = bucket_size_secs.max(LOSS_BUCKET_DURATION_SECS);

        // Filter to range
        let in_range: Vec<&LossBucket> = self
            .loss_archive
            .iter()
            .filter(|b| b.timestamp >= cutoff)
            .collect();

        if in_range.is_empty() {
            return Vec::new();
        }

        // Re-aggregate into requested bucket size
        let mut result: Vec<(i64, u64, u32)> = Vec::new();

        for bucket in in_range {
            let ts = bucket.timestamp.timestamp();
            let aligned_ts = ts - (ts % bucket_size);

            if let Some(last) = result.last_mut() {
                if last.0 == aligned_ts {
                    last.1 += bucket.total_loss;
                    last.2 += bucket.event_count;
                    continue;
                }
            }
            result.push((aligned_ts, bucket.total_loss, bucket.event_count));
        }

        result
    }

    /// Record a disconnection event
    ///
    /// # Arguments
    /// * `duration_ms` - Duration of the disconnection in milliseconds
    /// * `reconnected` - Whether reconnection was successful
    pub fn record_disconnection(&mut self, duration_ms: u64, reconnected: bool) {
        self.disconnection_events.push(DisconnectionEvent {
            timestamp: Utc::now(),
            duration_ms,
            reconnected,
        });
    }

    /// Get disconnection events
    pub fn disconnection_events(&self) -> &[DisconnectionEvent] {
        &self.disconnection_events
    }

    /// Get loss events
    pub fn loss_events(&self) -> &[LossEvent] {
        &self.loss_events
    }

    /// Get latency archive for extended history
    pub fn latency_archive(&self) -> &VecDeque<Measurement> {
        &self.latency_archive
    }

    /// Get extended latency plot data combining archive and recent history
    ///
    /// Returns up to `count` points, preferring recent full-resolution data
    /// and filling the rest from the down-sampled archive.
    ///
    /// # Returns
    /// Vector of (time_offset_seconds, latency_ms) pairs
    pub fn latency_plot_data_extended(&self, count: usize) -> Vec<(f64, f64)> {
        let now = Utc::now();
        let recent_count = self.latency_history.len().min(count);
        let archive_count = count.saturating_sub(recent_count);

        let mut data: Vec<(f64, f64)> = self
            .latency_archive
            .iter()
            .rev()
            .take(archive_count)
            .map(|m| {
                let time_offset = (now - m.timestamp).num_milliseconds() as f64 / 1000.0;
                (-time_offset, m.value)
            })
            .collect();

        let recent: Vec<(f64, f64)> = self
            .latency_history
            .iter()
            .rev()
            .take(recent_count)
            .map(|m| {
                let time_offset = (now - m.timestamp).num_milliseconds() as f64 / 1000.0;
                (-time_offset, m.value)
            })
            .collect();

        data.extend(recent);
        data
    }

    /// Set uptime seconds
    pub fn set_uptime(&mut self, seconds: u64) {
        self.stats.uptime_seconds = seconds;
    }

    /// Update device info (called from monitoring loop)
    pub fn set_device_info(
        &mut self,
        device_name: Option<String>,
        sample_rate: u32,
        buffer_size: u32,
    ) {
        self.stats.device_name = device_name;
        self.stats.sample_rate = sample_rate;
        self.stats.buffer_size = buffer_size;
    }

    /// Increment samples sent counter
    pub fn add_samples_sent(&mut self, count: u64) {
        self.stats.samples_sent += count;
    }

    /// Increment samples received counter
    pub fn add_samples_received(&mut self, count: u64) {
        self.stats.samples_received += count;
    }

    /// Get samples sent since reset
    pub fn samples_sent(&self) -> u64 {
        self.stats.samples_sent
    }

    /// Get samples received since reset
    pub fn samples_received(&self) -> u64 {
        self.stats.samples_received
    }

    /// Set samples sent counter (cumulative from engine)
    pub fn set_samples_sent(&mut self, count: u64) {
        self.stats.samples_sent = count;
    }

    /// Set samples received counter (cumulative from engine)
    pub fn set_samples_received(&mut self, count: u64) {
        self.stats.samples_received = count;
    }

    /// Set signal lost state
    pub fn set_signal_lost(&mut self, lost: bool) {
        self.stats.signal_lost = lost;
    }

    /// Get signal lost state
    pub fn signal_lost(&self) -> bool {
        self.stats.signal_lost
    }

    /// Set last confidence value
    pub fn set_confidence(&mut self, confidence: f32) {
        self.stats.last_confidence = confidence;
    }

    /// Get last confidence value
    pub fn confidence(&self) -> f32 {
        self.stats.last_confidence
    }

    /// Set counter silent state
    pub fn set_counter_silent(&mut self, silent: bool) {
        self.stats.counter_silent = silent;
    }

    /// Set estimated loss during counter silence
    pub fn set_estimated_loss(&mut self, estimated: u64) {
        self.stats.estimated_loss = estimated;
    }

    /// Reset estimated loss (called on recovery from silence or engine restart)
    pub fn reset_estimated_loss(&mut self) {
        self.stats.estimated_loss = 0;
        self.stats.counter_silent = false;
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

    #[test]
    fn test_set_sample_counters() {
        let mut store = StatsStore::new();

        // Set counters directly (cumulative from engine)
        store.set_samples_sent(1000);
        store.set_samples_received(999);

        assert_eq!(store.samples_sent(), 1000);
        assert_eq!(store.samples_received(), 999);

        // Overwrite with new cumulative values
        store.set_samples_sent(2500);
        store.set_samples_received(2490);

        assert_eq!(store.samples_sent(), 2500);
        assert_eq!(store.samples_received(), 2490);
    }
}
