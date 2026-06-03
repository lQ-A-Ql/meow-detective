/// Statistics collected during an ingestion run.
#[derive(Debug, Clone, Default)]
pub struct IngestStats {
    /// Total files enumerated.
    pub files_enumerated: u64,
    /// Total directories enumerated.
    pub dirs_enumerated: u64,
    /// Total bytes processed.
    pub bytes_processed: u64,
    /// Number of partitions detected.
    pub partitions_detected: u32,
    /// Number of partitions successfully processed.
    pub partitions_processed: u32,
    /// Number of timeline events generated.
    pub timeline_events: u64,
    /// Number of artifacts extracted.
    pub artifacts_extracted: u64,
    /// Number of warnings encountered.
    pub warning_count: u32,
    /// Number of files skipped.
    pub skipped_count: u32,
    /// Number of errors encountered.
    pub failed_count: u32,
}

impl IngestStats {
    pub fn merge(&mut self, other: &IngestStats) {
        self.files_enumerated += other.files_enumerated;
        self.dirs_enumerated += other.dirs_enumerated;
        self.bytes_processed += other.bytes_processed;
        self.partitions_detected += other.partitions_detected;
        self.partitions_processed += other.partitions_processed;
        self.timeline_events += other.timeline_events;
        self.artifacts_extracted += other.artifacts_extracted;
        self.warning_count += other.warning_count;
        self.skipped_count += other.skipped_count;
        self.failed_count += other.failed_count;
    }
}
