use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::{cluster_service, import_analysis, import_precheck, import_scheduler};
use domain::JobId;

use super::super::types::{BackgroundLinuxClusterImportJob, ClusterImportSummary};
use super::{AppHandle, CommandError, JobRepo};

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
}

pub(super) struct MemberCoordinator<'a, 'db> {
    pub(super) connection: &'a rusqlite::Connection,
    pub(super) job_repo: &'a JobRepo<'db>,
    pub(super) job: &'a BackgroundLinuxClusterImportJob,
    pub(super) app: Option<&'a AppHandle>,
    pub(super) cancel_token: &'a Arc<AtomicBool>,
    pub(super) total_members: u32,
}

pub(super) fn update_cluster_progress(
    connection: &rusqlite::Connection,
    job: &BackgroundLinuxClusterImportJob,
    summary: &ClusterImportSummary,
) -> Result<(), CommandError> {
    cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "importing",
        summary.ready_count,
        summary.failed_count,
        None,
    )
    .map_err(CommandError::from_typed_service_error)
}
