use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use crate::import_analysis::ImportAnalysisMode;
pub use crate::import_group_gate::{ImportGroupGateError, ImportGroupLease};
use crate::import_pipeline::{execute_import_job, ImportEventSink, ImportJobOptions};
use crate::import_precheck::ImportSourceConfig;
use persistence_sqlite::repositories::job_repo::JobRepo;
use transport::CommandError;

const IMPORT_GROUP_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

pub fn acquire_import_group_slot(
    case_id: &str,
    cancel_token: &AtomicBool,
) -> Result<ImportGroupLease, ImportGroupGateError> {
    crate::import_group_gate::global_import_group_gate().acquire(
        case_id,
        cancel_token,
        IMPORT_GROUP_WAIT_TIMEOUT,
    )
}

pub enum SingleImportOutcome {
    Completed(String),
    Cancelled(String),
}

pub struct SingleImportRequest<'a> {
    pub connection: &'a rusqlite::Connection,
    pub case_id: &'a domain::CaseId,
    pub case_root: &'a std::path::Path,
    pub job_id: &'a domain::JobId,
    pub import_config: ImportSourceConfig,
    pub max_import_workers: Option<usize>,
    pub max_analysis_workers: Option<usize>,
    pub analysis_mode: ImportAnalysisMode,
    pub cancel_token: &'a Arc<AtomicBool>,
    pub event_sink: Option<&'a dyn ImportEventSink>,
}

pub fn run_single_import(
    request: SingleImportRequest<'_>,
) -> Result<SingleImportOutcome, CommandError> {
    let SingleImportRequest {
        connection,
        case_id,
        case_root,
        job_id,
        import_config,
        max_import_workers,
        max_analysis_workers,
        analysis_mode,
        cancel_token,
        event_sink,
    } = request;
    let repo = JobRepo::new(connection);
    if cancel_token.load(std::sync::atomic::Ordering::Acquire) {
        repo.cancel(job_id, "Import cancelled by user")
            .map_err(CommandError::from_typed_service_error)?;
        return Ok(SingleImportOutcome::Cancelled(
            "Import cancelled by user".to_string(),
        ));
    }
    let _group_slot =
        acquire_import_group_slot(&case_id.0, cancel_token.as_ref()).map_err(|error| {
            CommandError::timeout(format!("Import group admission failed: {error:?}"))
        })?;
    let scheduling = crate::import_scheduler::ImportSchedulingPolicy::for_workload(
        crate::import_scheduler::ImportWorkload::SingleSource,
        max_import_workers,
        max_analysis_workers,
    );
    let _admission = crate::import_scheduler::global_import_admission()
        .acquire(scheduling.admission_request(), cancel_token.as_ref())
        .map_err(|error| match error {
            crate::import_scheduler::ImportAdmissionError::Cancelled => {
                CommandError::cancelled("Import cancelled while waiting for scheduler admission")
            }
            crate::import_scheduler::ImportAdmissionError::TimedOut => {
                CommandError::timeout("Import scheduler admission timed out")
            }
        })?;
    if !repo
        .start(job_id)
        .map_err(CommandError::from_typed_service_error)?
    {
        repo.cancel(job_id, "Import cancelled before execution")
            .map_err(CommandError::from_typed_service_error)?;
        return Ok(SingleImportOutcome::Cancelled(
            "Import cancelled before execution".to_string(),
        ));
    }
    if let Some(event_sink) = event_sink {
        event_sink.job_progress(&job_id.0, 5, "Import started");
    }
    let options = ImportJobOptions {
        projection_policy: Default::default(),
        event_sink,
        cancel_token,
        max_import_workers: Some(scheduling.import_workers),
        max_analysis_workers: Some(scheduling.analysis_workers),
        analysis_mode,
        defer_linux_artifacts: false,
    };
    match execute_import_job(
        connection,
        case_id,
        case_root,
        import_config,
        job_id,
        options,
    ) {
        Ok(message) => {
            repo.complete_committed(job_id, &message)
                .map_err(CommandError::from_typed_service_error)?;
            Ok(SingleImportOutcome::Completed(message))
        }
        Err(error)
            if error.category == "cancelled"
                || cancel_token.load(std::sync::atomic::Ordering::Acquire) =>
        {
            repo.cancel(job_id, &error.message)
                .map_err(CommandError::from_typed_service_error)?;
            Ok(SingleImportOutcome::Cancelled(error.message))
        }
        Err(error) => {
            repo.fail(job_id, &error.message)
                .map_err(CommandError::from_typed_service_error)?;
            Err(error)
        }
    }
}
