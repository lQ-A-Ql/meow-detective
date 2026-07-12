#[derive(Debug, Clone, Default)]
pub(super) struct WorkerStats {
    pub(super) processed_count: u64,
    pub(super) artifact_count: u64,
    pub(super) timeline_count: u64,
    pub(super) indexed_count: u64,
    pub(super) warning_count: u32,
    pub(super) skipped_count: u32,
    pub(super) failed_count: u32,
}
