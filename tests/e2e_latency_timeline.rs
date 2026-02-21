//! E2E tests for latency timeline archive and bucketed query
//!
//! Verifies that latency measurements are aggregated into 10-second buckets,
//! stored in a 14-day rolling archive, and queryable via latency_timeline_data.

use audiotester::stats::store::StatsStore;

/// Test LatencyBucket is created when recording latency
#[test]
fn test_latency_bucket_archive_records_bucket() {
    let mut store = StatsStore::new();

    store.record_latency(5.0);

    let buckets = store.latency_timeline_data(3600, 10);
    assert!(
        !buckets.is_empty(),
        "Should have at least one bucket after recording latency"
    );

    let last = buckets.last().unwrap();
    // (unix_ts, avg, min, max)
    assert!(
        (last.1 - 5.0).abs() < 0.001,
        "Bucket avg should be 5.0, got {}",
        last.1
    );
    assert!(
        (last.2 - 5.0).abs() < 0.001,
        "Bucket min should be 5.0, got {}",
        last.2
    );
    assert!(
        (last.3 - 5.0).abs() < 0.001,
        "Bucket max should be 5.0, got {}",
        last.3
    );
}

/// Test multiple latencies within same bucket are aggregated correctly
#[test]
fn test_latency_bucket_archive_aggregates_within_bucket() {
    let mut store = StatsStore::new();

    // Record multiple latencies rapidly (within same 10s bucket)
    store.record_latency(3.0);
    store.record_latency(5.0);
    store.record_latency(7.0);

    let buckets = store.latency_timeline_data(3600, 10);
    assert!(!buckets.is_empty());

    let last = buckets.last().unwrap();
    // avg should be (3+5+7)/3 = 5.0
    assert!(
        (last.1 - 5.0).abs() < 0.001,
        "Bucket avg should be 5.0, got {}",
        last.1
    );
    assert!(
        (last.2 - 3.0).abs() < 0.001,
        "Bucket min should be 3.0, got {}",
        last.2
    );
    assert!(
        (last.3 - 7.0).abs() < 0.001,
        "Bucket max should be 7.0, got {}",
        last.3
    );
}

/// Test latency_timeline_data re-aggregates into larger buckets with weighted avg
#[test]
fn test_latency_timeline_reaggregation() {
    let mut store = StatsStore::new();

    // Record latencies
    store.record_latency(10.0);
    store.record_latency(20.0);

    // Query with 10s bucket size
    let buckets_10s = store.latency_timeline_data(3600, 10);
    // Query with 60s bucket size (should merge 10s buckets)
    let buckets_60s = store.latency_timeline_data(3600, 60);

    // 60s buckets should have fewer or equal entries
    assert!(
        buckets_60s.len() <= buckets_10s.len(),
        "60s buckets ({}) should be <= 10s buckets ({})",
        buckets_60s.len(),
        buckets_10s.len()
    );

    // Average should be preserved when all data is in one bucket
    if !buckets_60s.is_empty() {
        let avg_60s = buckets_60s[0].1;
        assert!(
            (avg_60s - 15.0).abs() < 0.001,
            "60s bucket avg should be 15.0 (weighted avg of 10+20), got {}",
            avg_60s
        );
    }
}

/// Test clear() also clears latency_bucket_archive
#[test]
fn test_clear_clears_latency_bucket_archive() {
    let mut store = StatsStore::new();

    store.record_latency(5.0);
    assert!(!store.latency_timeline_data(3600, 10).is_empty());

    store.clear();
    assert!(
        store.latency_timeline_data(3600, 10).is_empty(),
        "clear() should also clear latency_bucket_archive"
    );
}

/// Test reset_counters preserves latency_bucket_archive
#[test]
fn test_reset_counters_preserves_latency_bucket_archive() {
    let mut store = StatsStore::new();

    store.record_latency(5.0);
    let buckets_before = store.latency_timeline_data(3600, 10);
    assert!(!buckets_before.is_empty());

    store.reset_counters();

    let buckets_after = store.latency_timeline_data(3600, 10);
    assert_eq!(
        buckets_before.len(),
        buckets_after.len(),
        "reset_counters should preserve latency_bucket_archive"
    );
}

/// Test latency_timeline_data returns unix timestamps
#[test]
fn test_latency_timeline_returns_unix_timestamps() {
    let mut store = StatsStore::new();

    store.record_latency(5.0);

    let buckets = store.latency_timeline_data(3600, 10);
    assert!(!buckets.is_empty());

    let timestamp = buckets[0].0;
    // Unix timestamp should be a reasonable value (after 2020)
    assert!(
        timestamp > 1_577_836_800,
        "Timestamp {} should be after 2020",
        timestamp
    );
}

/// Test range filtering in latency_timeline_data
#[test]
fn test_latency_timeline_range_filtering() {
    let mut store = StatsStore::new();

    store.record_latency(10.0);

    // 1h range should include recent data
    let buckets_1h = store.latency_timeline_data(3600, 10);
    assert!(
        !buckets_1h.is_empty(),
        "1h range should include recent data"
    );

    // 14d range should also include it
    let buckets_14d = store.latency_timeline_data(1_209_600, 3600);
    assert!(
        !buckets_14d.is_empty(),
        "14d range should include recent data"
    );
}
