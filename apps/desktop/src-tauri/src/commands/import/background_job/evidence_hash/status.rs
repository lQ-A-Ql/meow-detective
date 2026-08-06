use app_services::hash_service::{
    evidence_jobs::{cancel_hash_job, complete_hash_job, fail_hash_job, EvidenceHashJobError},
    EvidenceHashError, EvidenceHashResult,
};
use domain::{DataSourceId, JobId};
use tauri::AppHandle;
use transport::dto::{
    CancellationStateDto, PartialResultDto, PartialResultKindDto, ResultFreshnessDto,
};
use transport::CommandError;

use super::super::super::cancellation::job_cancellation_dto;
use crate::events::event_bridge;

pub(super) fn complete_hash(
    connection: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    app: Option<&AppHandle>,
    result: &EvidenceHashResult,
) -> Result<(), CommandError> {
    let detail = complete_hash_job(connection, data_source_id, job_id, result)
        .map_err(CommandError::from_typed_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 100, &detail);
        event_bridge::emit_job_completed(app, &job_id.0, &detail);
        event_bridge::emit_import_partial_result(
            app,
            &PartialResultDto {
                kind: PartialResultKindDto::EvidenceHash,
                scope_id: data_source_id.0.clone(),
                ready_count: 1,
                total_estimate: Some(1),
                query_key: "case.metrics".to_string(),
                freshness: ResultFreshnessDto::Ready,
            },
        );
    }
    Ok(())
}

pub(super) fn fail_hash(
    connection: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    app: Option<&AppHandle>,
    error: EvidenceHashError,
) -> Result<(), CommandError> {
    let detail = format!("Evidence SHA-256 failed: {error}");
    settle_failed_job(connection, data_source_id, job_id, app, &detail)?;
    Err(CommandError::internal(detail))
}

pub(super) fn fail_hash_setup(
    connection: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    app: Option<&AppHandle>,
    error: EvidenceHashJobError,
) -> Result<(), CommandError> {
    tracing::warn!(data_source_id = %data_source_id.0, %error, "Failed to load evidence source for background hashing");
    let detail = "Evidence SHA-256 setup failed";
    settle_failed_job(connection, data_source_id, job_id, app, detail)?;
    Err(CommandError::internal(detail))
}

fn settle_failed_job(
    connection: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    app: Option<&AppHandle>,
    detail: &str,
) -> Result<(), CommandError> {
    fail_hash_job(connection, data_source_id, job_id, detail)
        .map_err(CommandError::from_typed_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_failed(app, &job_id.0, detail);
    }
    Ok(())
}

pub(super) fn cancel_hash(
    connection: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    app: Option<&AppHandle>,
) -> Result<(), CommandError> {
    let detail = "Evidence hash cancelled by user";
    let changed = cancel_hash_job(connection, data_source_id, job_id, detail)
        .map_err(CommandError::from_typed_service_error)?;
    if changed {
        if let Some(app) = app {
            event_bridge::emit_job_cancelled(app, &job_id.0, detail);
            event_bridge::emit_job_cancellation(
                app,
                &job_cancellation_dto(&job_id.0, CancellationStateDto::Cancelled, true, detail),
            );
        }
    }
    Ok(())
}
