use transport::dto::{CorrelationSnapshotDto, V2GovernanceSnapshotDto};

#[derive(Clone, Copy)]
pub struct BitLockerReportContext<'a> {
    pub(crate) runtimes: crate::bitlocker_service::BitLockerRuntimeContext<'a>,
}

impl<'a> BitLockerReportContext<'a> {
    #[must_use]
    pub fn new(runtimes: crate::bitlocker_service::BitLockerRuntimeContext<'a>) -> Self {
        Self { runtimes }
    }
}

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
