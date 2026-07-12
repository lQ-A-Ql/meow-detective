use transport::dto::{CorrelationSnapshotDto, V2GovernanceSnapshotDto};

pub(crate) struct ReportCorrelation {
    pub(crate) snapshot: CorrelationSnapshotDto,
}

pub(crate) struct ReportGovernance {
    pub(crate) snapshot: V2GovernanceSnapshotDto,
}

pub(crate) struct RawExportBundle {
    pub(crate) bundle_dir_name: String,
    pub(crate) manifest_file_name: String,
    pub(crate) hashes_file_name: String,
    pub(crate) exported_count: usize,
    pub(crate) skipped_count: usize,
    pub(crate) skipped_files: Vec<String>,
}
