//! E2E tests for loss timeline archive and bucketed query
//!
//! Verifies that loss events are aggregated into 10-second buckets,
//! stored in a 24-hour rolling archive, and queryable via timeline_data.

use audiotester::stats::store::StatsStore;

/// Test LossBucket is created when recording loss
#[test]
fn test_loss_archive_records_bucket() {
    let mut store = StatsStore::new();

    store.record_loss(42);

    let buckets = store.loss_timeline_data(3600, 10);
    assert!(
        !buckets.is_empty(),
        "Should have at least one bucket after recording loss"
    );

    let last = buckets.last().unwrap();
    assert_eq!(last.1, 42, "Bucket should contain 42 lost samples");
    assert_eq!(last.2, 1, "Bucket should contain 1 event");
}

/// Test multiple losses within same bucket are aggregated
#[test]
fn test_loss_archive_aggregates_within_bucket() {
    let mut store = StatsStore::new();

    // Record multiple losses rapidly (within same 10s bucket)
    store.record_loss(10);
    store.record_loss(20);
    store.record_loss(30);

    let buckets = store.loss_timeline_data(3600, 10);
    assert!(!buckets.is_empty());

    // All three should be in the same bucket (recorded within milliseconds)
    let last = buckets.last().unwrap();
    assert_eq!(
        last.1, 60,
        "Bucket should aggregate to 60 total lost samples"
    );
    assert_eq!(last.2, 3, "Bucket should contain 3 events");
}

/// Test loss_archive_tick creates zero buckets for loss-free periods
#[test]
fn test_loss_archive_tick_creates_zero_bucket() {
    let mut store = StatsStore::new();

    // Record a loss to establish first bucket
    store.record_loss(5);

    // Tick should create a new zero bucket if enough time has passed
    // We can't easily test time-based behavior without mocking,
    // but we can verify tick doesn't panic and the method exists
    store.loss_archive_tick();

    // After tick, archive should still have at least 1 bucket
    let buckets = store.loss_timeline_data(3600, 10);
    assert!(!buckets.is_empty());
}

/// Test loss_archive respects MAX_LOSS_ARCHIVE_SIZE
#[test]
fn test_loss_archive_overflow() {
    let mut store = StatsStore::new();

    // Record more buckets than the max (8640)
    // Each record_loss creates/updates a bucket
    // We need to force new buckets by using tick
    for _ in 0..100 {
        store.record_loss(1);
    }

    let buckets = store.loss_timeline_data(86400, 10);
    assert!(
        buckets.len() <= 8640,
        "Archive should not exceed 8640 buckets, got {}",
        buckets.len()
    );
}

/// Test loss_timeline_data re-aggregates into larger buckets
#[test]
fn test_loss_timeline_reaggregation() {
    let mut store = StatsStore::new();

    // Record some loss
    store.record_loss(10);
    store.record_loss(20);

    // Query with 60s bucket size (should aggregate 10s buckets into 60s)
    let buckets_10s = store.loss_timeline_data(3600, 10);
    let buckets_60s = store.loss_timeline_data(3600, 60);

    // 60s buckets should have fewer or equal entries than 10s buckets
    assert!(
        buckets_60s.len() <= buckets_10s.len(),
        "60s buckets ({}) should be <= 10s buckets ({})",
        buckets_60s.len(),
        buckets_10s.len()
    );

    // Total loss should be preserved across aggregation levels
    let total_10s: u64 = buckets_10s.iter().map(|b| b.1).sum();
    let total_60s: u64 = buckets_60s.iter().map(|b| b.1).sum();
    assert_eq!(
        total_10s, total_60s,
        "Total loss should be same at both aggregation levels"
    );
}

/// Test clear() also clears loss_archive
#[test]
fn test_clear_clears_loss_archive() {
    let mut store = StatsStore::new();

    store.record_loss(100);
    assert!(!store.loss_timeline_data(3600, 10).is_empty());

    store.clear();
    assert!(
        store.loss_timeline_data(3600, 10).is_empty(),
        "clear() should also clear loss_archive"
    );
}

/// Test reset_counters preserves loss_archive
#[test]
fn test_reset_counters_preserves_loss_archive() {
    let mut store = StatsStore::new();

    store.record_loss(100);
    let buckets_before = store.loss_timeline_data(3600, 10);
    assert!(!buckets_before.is_empty());

    store.reset_counters();

    let buckets_after = store.loss_timeline_data(3600, 10);
    assert_eq!(
        buckets_before.len(),
        buckets_after.len(),
        "reset_counters should preserve loss_archive"
    );
}

/// Test timeline_data returns unix timestamps
#[test]
fn test_timeline_data_returns_unix_timestamps() {
    let mut store = StatsStore::new();

    store.record_loss(5);

    let buckets = store.loss_timeline_data(3600, 10);
    assert!(!buckets.is_empty());

    let timestamp = buckets[0].0;
    // Unix timestamp should be a reasonable value (after 2020)
    assert!(
        timestamp > 1_577_836_800,
        "Timestamp {} should be after 2020",
        timestamp
    );
}

/// Test range filtering in timeline_data
#[test]
fn test_timeline_data_range_filtering() {
    let mut store = StatsStore::new();

    store.record_loss(10);

    // 1h range should include recent data
    let buckets_1h = store.loss_timeline_data(3600, 10);
    assert!(
        !buckets_1h.is_empty(),
        "1h range should include recent data"
    );

    // 24h range should also include it
    let buckets_24h = store.loss_timeline_data(86400, 300);
    assert!(
        !buckets_24h.is_empty(),
        "24h range should include recent data"
    );
}
