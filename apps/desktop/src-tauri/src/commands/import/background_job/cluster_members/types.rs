use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::{cluster_service, import_analysis, import_precheck, import_scheduler};
use domain::JobId;

use super::super::types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary};
use super::{AppHandle, CommandError, JobRepo};
use crate::state::TaskManager;

pub(super) enum MemberFailureAction {
    Continue,
    StopCancelled,
}

pub(super) struct MemberWork {
    pub(super) index: usize,
    pub(super) config: import_precheck::ImportSourceConfig,
    pub(super) job_id: JobId,
}

pub(super) struct MemberResult {
    pub(super) index: usize,
    pub(super) job_id: JobId,
    pub(super) outcome: Result<String, CommandError>,
}

pub(super) struct MemberExecutionContext {
    pub(super) db_path: PathBuf,
    pub(super) case_id: domain::CaseId,
    pub(super) case_root: PathBuf,
    pub(super) app: Option<AppHandle>,
    pub(super) cancel_token: Arc<AtomicBool>,
    pub(super) scheduling: import_scheduler::ImportSchedulingPolicy,
    pub(super) analysis_mode: import_analysis::ImportAnalysisMode,
    pub(super) defer_linux_artifacts: bool,
}

pub(in crate::commands::import::background_job) struct MemberCoordinator<'a, 'db> {
    pub(in crate::commands::import::background_job) connection: &'a rusqlite::Connection,
    pub(in crate::commands::import::background_job) job_repo: &'a JobRepo<'db>,
    pub(in crate::commands::import::background_job) job: &'a BackgroundLinuxEvidenceSetImportJob,
    pub(in crate::commands::import::background_job) app: Option<&'a AppHandle>,
    pub(in crate::commands::import::background_job) cancel_token: &'a Arc<AtomicBool>,
    pub(in crate::commands::import::background_job) total_members: u32,
    pub(in crate::commands::import::background_job) task_manager: Option<Arc<TaskManager>>,
}

pub(super) fn update_cluster_progress(
    connection: &rusqlite::Connection,
    job: &BackgroundLinuxEvidenceSetImportJob,
    summary: &EvidenceSetImportSummary,
) -> Result<(), CommandError> {
    cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "importing",
        summary.ready_count,
        summary.failed_count,
        None,
    )
    .map_err(CommandError::from_typed_service_error)
}
