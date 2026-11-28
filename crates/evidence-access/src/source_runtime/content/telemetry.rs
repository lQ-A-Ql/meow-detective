use evidence_core::reader::EvidenceReadMetrics;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub(super) struct ScanTelemetry {
    pub admission_nanos: u64,
    pub cpu_wait_nanos: AtomicU64,
    pub read_nanos: AtomicU64,
    pub chunk_hash_nanos: AtomicU64,
    pub send_nanos: AtomicU64,
    pub receive_nanos: AtomicU64,
    pub ordered_hash_nanos: AtomicU64,
    pub logical_read_bytes: AtomicU64,
    pub completed_frames: AtomicU64,
    pub reader: std::sync::Mutex<EvidenceReadMetrics>,
}

impl ScanTelemetry {
    pub fn record(counter: &AtomicU64, started: std::time::Instant) {
        counter.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn report(&self) {
        let reader = *self.reader.lock().expect("scan reader metrics lock");
        tracing::info!(
            admission_ms = self.admission_nanos / 1_000_000,
            cpu_wait_ms = self.cpu_wait_nanos.load(Ordering::Relaxed) / 1_000_000,
            read_ms = self.read_nanos.load(Ordering::Relaxed) / 1_000_000,
            chunk_hash_ms = self.chunk_hash_nanos.load(Ordering::Relaxed) / 1_000_000,
            send_wait_ms = self.send_nanos.load(Ordering::Relaxed) / 1_000_000,
            receive_wait_ms = self.receive_nanos.load(Ordering::Relaxed) / 1_000_000,
            ordered_hash_ms = self.ordered_hash_nanos.load(Ordering::Relaxed) / 1_000_000,
            logical_read_bytes = self.logical_read_bytes.load(Ordering::Relaxed),
            completed_frames = self.completed_frames.load(Ordering::Relaxed),
            stored_reads = reader.stored_reads,
            stored_bytes = reader.stored_bytes,
            decoded_chunks = reader.decoded_chunks,
            decoded_bytes = reader.decoded_bytes,
            metadata_checks = reader.metadata_checks,
            index_ms = reader.index_nanos / 1_000_000,
            io_ms = reader.io_nanos / 1_000_000,
            decode_ms = reader.decode_nanos / 1_000_000,
            "Content scan stage measurements"
        );
    }
}
