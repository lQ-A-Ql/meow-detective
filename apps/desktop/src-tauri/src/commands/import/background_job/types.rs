use std::path::PathBuf;

use app_services::{cluster_service, import_analysis, import_precheck};

use crate::state::AppState;

pub(crate) struct BackgroundImportJob {
    pub(crate) db_path: PathBuf,
    pub(crate) case_id: domain::CaseId,
    pub(crate) case_root: PathBuf,
    pub(crate) import_config: import_precheck::ImportSourceConfig,
    pub(crate) job_id: domain::JobId,
    pub(crate) max_import_workers: Option<usize>,
    pub(crate) max_analysis_workers: Option<usize>,
    pub(crate) analysis_mode: import_analysis::ImportAnalysisMode,
    /// App state captured at scheduling time so the post-import scheduler can
    /// fence against the active case after the job thread finishes.
    /// `AppState` itself is cheap to clone (all fields are `Arc`).
    pub(crate) app_state: AppState,
}

pub(crate) struct BackgroundLinuxEvidenceSetImportJob {
    pub(crate) db_path: PathBuf,
    pub(crate) case_id: domain::CaseId,
    pub(crate) case_root: PathBuf,
    pub(crate) plan: cluster_service::LinuxEvidenceSetImportPlan,
    pub(crate) job_id: domain::JobId,
    pub(crate) max_import_workers: Option<usize>,
    pub(crate) max_analysis_workers: Option<usize>,
    pub(crate) analysis_mode: import_analysis::ImportAnalysisMode,
}

pub(crate) struct BackgroundDerivedSourceProcessingJob {
    pub(crate) db_path: PathBuf,
    pub(crate) case_id: domain::CaseId,
    pub(crate) case_root: PathBuf,
    pub(crate) import_set_id: String,
    pub(crate) source_ids: Vec<domain::DataSourceId>,
}

pub(crate) struct BrowseableEvidenceSetImport {
    pub(crate) processing: BackgroundDerivedSourceProcessingJob,
    pub(crate) parent_job_id: domain::JobId,
    pub(crate) completion_detail: String,
}

pub(super) struct EvidenceSetImportSummary {
    pub(super) collection_plan:
        Option<std::sync::Arc<app_services::datasource_service::ImageCollectionPlan>>,
    pub(super) ready_count: u32,
    pub(super) failed_count: u32,
    pub(super) member_messages: Vec<String>,
}

impl EvidenceSetImportSummary {
    pub(super) fn new() -> Self {
        Self {
            collection_plan: None,
            ready_count: 0,
            failed_count: 0,
            member_messages: Vec::new(),
        }
    }
}
