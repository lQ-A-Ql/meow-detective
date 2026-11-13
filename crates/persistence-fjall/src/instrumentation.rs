//! Structural I/O counters for Fjall storage benchmarks.
//!
//! Thread-local counters support focused storage assertions. A process measurement
//! collects joined worker activity for an isolated benchmark execution.

use std::{
    cell::Cell,
    sync::atomic::{AtomicBool, Ordering},
};

mod process;
pub use process::{MeasurementBusy, ProcessMeasurement};

/// Structural storage work recorded since the last [`reset`] on this thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StorageCounters {
    /// Single-key reads that returned a value.
    pub point_reads: u64,
    /// Scan materializations (prefix, range, keyset, range_keyset).
    pub scan_calls: u64,
    /// Entries materialized by scans.
    pub scan_entries: u64,
    /// Payload bytes returned by point reads and scans.
    pub bytes_read: u64,
    /// Committed write batches.
    pub write_batches: u64,
    /// Write actions carried by committed batches.
    pub written_actions: u64,
    /// Key plus value bytes carried by committed batches.
    pub bytes_written: u64,
    /// Persist calls and committed batches using SyncAll durability.
    pub sync_all_persists: u64,
    /// Database opens.
    pub database_opens: u64,
    pub search_index_opens: u64,
    pub query_session_opens: u64,
    pub source_session_opens: u64,
    pub path_normalizations: u64,
}

static ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static COUNTERS: Cell<StorageCounters> = Cell::new(StorageCounters::default());
}

/// Starts recording on the calling thread and on every other thread.
pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// Stops all recording.
pub fn disable() {
    ENABLED.store(false, Ordering::Relaxed);
}

/// Zeroes only the calling thread's counters.
pub fn reset() {
    COUNTERS.with(|counters| counters.set(StorageCounters::default()));
}

/// Returns the calling thread's counters.
pub fn snapshot() -> StorageCounters {
    COUNTERS.with(|counters| counters.get())
}

fn apply(update: impl Fn(&mut StorageCounters)) {
    if ENABLED.load(Ordering::Relaxed) {
        COUNTERS.with(|counters| {
            let mut current = counters.get();
            update(&mut current);
            counters.set(current);
        });
    }
    process::record(update);
}

pub(crate) fn record_point_read(bytes: usize) {
    apply(|counters| {
        counters.point_reads += 1;
        counters.bytes_read += bytes as u64;
    });
}

pub(crate) fn record_scan(entries: usize, bytes: usize) {
    apply(|counters| {
        counters.scan_calls += 1;
        counters.scan_entries += entries as u64;
        counters.bytes_read += bytes as u64;
    });
}

pub(crate) fn record_write_batch(actions: usize, bytes: usize, sync_all: bool) {
    apply(|counters| {
        counters.write_batches += 1;
        counters.written_actions += actions as u64;
        counters.bytes_written += bytes as u64;
        if sync_all {
            counters.sync_all_persists += 1;
        }
    });
}

pub(crate) fn record_persist(sync_all: bool) {
    if sync_all {
        apply(|counters| counters.sync_all_persists += 1);
    }
}

pub(crate) fn record_database_open() {
    apply(|counters| counters.database_opens += 1);
}

pub fn record_search_index_open() {
    apply(|counters| counters.search_index_opens += 1);
}

pub fn record_query_session_open() {
    apply(|counters| counters.query_session_opens += 1);
}

pub fn record_source_session_open() {
    apply(|counters| counters.source_session_opens += 1);
}

pub fn record_path_normalization() {
    apply(|counters| counters.path_normalizations += 1);
}
