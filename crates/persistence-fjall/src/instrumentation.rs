//! Structural I/O counters for Fjall storage benchmarks.
//!
//! Recording is gated by one process-wide relaxed atomic load, so the disabled
//! path costs a single branch and never touches a thread-local. Counters are
//! thread-local: work executed on other threads (for example Rayon workers) is
//! not attributed to the thread that called [`enable`].

use std::{
    cell::Cell,
    sync::atomic::{AtomicBool, Ordering},
};

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

fn apply(update: impl FnOnce(&mut StorageCounters)) {
    COUNTERS.with(|counters| {
        let mut current = counters.get();
        update(&mut current);
        counters.set(current);
    });
}

pub(crate) fn record_point_read(bytes: usize) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    apply(|counters| {
        counters.point_reads += 1;
        counters.bytes_read += bytes as u64;
    });
}

pub(crate) fn record_scan(entries: usize, bytes: usize) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    apply(|counters| {
        counters.scan_calls += 1;
        counters.scan_entries += entries as u64;
        counters.bytes_read += bytes as u64;
    });
}

pub(crate) fn record_write_batch(actions: usize, bytes: usize, sync_all: bool) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
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
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if sync_all {
        apply(|counters| counters.sync_all_persists += 1);
    }
}

pub(crate) fn record_database_open() {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    apply(|counters| counters.database_opens += 1);
}
