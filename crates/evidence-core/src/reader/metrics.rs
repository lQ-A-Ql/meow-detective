#[derive(Debug, Clone, Copy, Default)]
pub struct EvidenceReadMetrics {
    pub stored_reads: u64,
    pub stored_bytes: u64,
    pub decoded_chunks: u64,
    pub decoded_bytes: u64,
    pub metadata_checks: u64,
    pub index_nanos: u64,
    pub io_nanos: u64,
    pub decode_nanos: u64,
}

impl EvidenceReadMetrics {
    pub fn merge(&mut self, other: Self) {
        self.stored_reads += other.stored_reads;
        self.stored_bytes += other.stored_bytes;
        self.decoded_chunks += other.decoded_chunks;
        self.decoded_bytes += other.decoded_bytes;
        self.metadata_checks += other.metadata_checks;
        self.index_nanos += other.index_nanos;
        self.io_nanos += other.io_nanos;
        self.decode_nanos += other.decode_nanos;
    }
}
