use std::path::PathBuf;

use app_services::{cluster_service, import_analysis, import_precheck};

pub(crate) struct BackgroundImportJob {
    pub(crate) db_path: PathBuf,
    pub(crate) case_id: domain::CaseId,
    pub(crate) case_root: PathBuf,
    pub(crate) import_config: import_precheck::ImportSourceConfig,
    pub(crate) job_id: domain::JobId,
    pub(crate) max_import_workers: Option<usize>,
    pub(crate) max_analysis_workers: Option<usize>,
    pub(crate) analysis_mode: import_analysis::ImportAnalysisMode,
}

pub(crate) struct BackgroundLinuxClusterImportJob {
    pub(crate) db_path: PathBuf,
    pub(crate) case_id: domain::CaseId,
    pub(crate) case_root: PathBuf,
    pub(crate) plan: cluster_service::LinuxClusterImportPlan,
    pub(crate) job_id: domain::JobId,
    pub(crate) max_import_workers: Option<usize>,
    pub(crate) max_analysis_workers: Option<usize>,
    pub(crate) analysis_mode: import_analysis::ImportAnalysisMode,
}

pub(super) struct ClusterImportSummary {
    pub(super) ready_count: u32,
    pub(super) failed_count: u32,
    pub(super) member_messages: Vec<String>,
}

impl ClusterImportSummary {
    pub(super) fn new() -> Self {
        Self {
            ready_count: 0,
            failed_count: 0,
            member_messages: Vec::new(),
        }
    }
}
