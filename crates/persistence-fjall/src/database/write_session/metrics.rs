use super::*;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeWriteReport {
    pub configured_workers: usize,
    pub workers_used: u32,
    pub peak_native_calls: usize,
    pub native_calls: u64,
    pub batches: u64,
    pub actions: u64,
    pub encoded_bytes: u64,
    pub elapsed_micros: u64,
    pub finish_micros: u64,
}

#[derive(Default)]
pub(in crate::database) struct NativeWriteMetrics {
    active: AtomicUsize,
    peak: AtomicUsize,
    workers: AtomicU64,
    calls: AtomicU64,
    pub(in crate::database) batches: AtomicU64,
    pub(in crate::database) actions: AtomicU64,
    pub(in crate::database) bytes: AtomicU64,
    pub(in crate::database) finish_micros: AtomicU64,
}

pub(in crate::database) struct NativeCall<'a>(&'a NativeWriteMetrics);

impl NativeWriteMetrics {
    pub(in crate::database) fn enter(&self, worker: usize) -> NativeCall<'_> {
        self.workers.fetch_or(1u64 << worker, Ordering::Relaxed);
        let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.peak.fetch_max(active, Ordering::Relaxed);
        self.calls.fetch_add(1, Ordering::Relaxed);
        NativeCall(self)
    }

    pub(super) fn report(&self, workers: usize, elapsed: Duration) -> NativeWriteReport {
        NativeWriteReport {
            configured_workers: workers,
            workers_used: self.workers.load(Ordering::Relaxed).count_ones(),
            peak_native_calls: self.peak.load(Ordering::Relaxed),
            native_calls: self.calls.load(Ordering::Relaxed),
            batches: self.batches.load(Ordering::Relaxed),
            actions: self.actions.load(Ordering::Relaxed),
            encoded_bytes: self.bytes.load(Ordering::Relaxed),
            elapsed_micros: elapsed.as_micros().try_into().unwrap_or(u64::MAX),
            finish_micros: self.finish_micros.load(Ordering::Relaxed),
        }
    }
}

impl Drop for NativeCall<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}
