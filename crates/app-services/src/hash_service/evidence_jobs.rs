use domain::{CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, JobId};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use rusqlite::Connection;
use transport::{ErrorCategory, ServiceErrorCategory};

use super::EvidenceHashResult;

pub const EVIDENCE_HASH_JOB_KIND: &str = "Evidence SHA-256";

#[derive(Debug, thiserror::Error)]
pub enum EvidenceHashJobError {
    #[error("evidence hash job database operation failed")]
    Database(#[source] persistence_sqlite::DbError),
    #[error("evidence hash data source was not found")]
    NotFound,
}

impl From<persistence_sqlite::DbError> for EvidenceHashJobError {
    fn from(error: persistence_sqlite::DbError) -> Self {
        tracing::error!(%error, "Evidence hash job database operation failed");
        Self::Database(error)
    }
}

impl From<rusqlite::Error> for EvidenceHashJobError {
    fn from(error: rusqlite::Error) -> Self {
        persistence_sqlite::DbError::from(error).into()
    }
}

impl ServiceErrorCategory for EvidenceHashJobError {
    fn category(&self) -> ErrorCategory {
        match self {
            Self::Database(_) => ErrorCategory::Io,
            Self::NotFound => ErrorCategory::Validation,
        }
    }
}

pub fn list_pending_hash_sources(
    connection: &Connection,
    case_id: &CaseId,
) -> Result<Vec<DataSourceId>, EvidenceHashJobError> {
    let repo = DataSourceRepo::new(connection);
    let mut candidates = Vec::new();
    for source in repo.find_by_case(case_id)? {
        if !is_hash_candidate(&source) || !is_ready(&repo, &source.id)? {
            continue;
        }
        candidates.push(source.id);
    }
    Ok(candidates)
}

pub fn create_hash_job_if_absent(
    connection: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<Option<JobId>, EvidenceHashJobError> {
    let transaction = connection.unchecked_transaction()?;
    let source = load_source(&transaction, data_source_id)?;
    let source_repo = DataSourceRepo::new(&transaction);
    if !is_hash_candidate(&source) || !is_ready(&source_repo, data_source_id)? {
        transaction.commit()?;
        return Ok(None);
    }
    let job_repo = JobRepo::new(&transaction);
    let prefix = detail_prefix(data_source_id);
    if job_repo.has_active_kind_with_detail_prefix(EVIDENCE_HASH_JOB_KIND, &prefix)? {
        transaction.commit()?;
        return Ok(None);
    }
    let job_id = job_repo.create(&case_id.0, EVIDENCE_HASH_JOB_KIND)?;
    job_repo.update_progress(&job_id, 1, &format!("{prefix}queued"))?;
    transaction.commit()?;
    Ok(Some(job_id))
}

pub fn load_hash_source(
    connection: &Connection,
    data_source_id: &DataSourceId,
) -> Result<DataSource, EvidenceHashJobError> {
    load_source(connection, data_source_id)
}

pub fn update_hash_progress(
    connection: &Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    percent: u32,
) -> Result<(), EvidenceHashJobError> {
    let detail = format!("{}hashing {percent}%", detail_prefix(data_source_id));
    JobRepo::new(connection).update_progress(job_id, percent, &detail)?;
    Ok(())
}

pub fn complete_hash_job(
    connection: &Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    result: &EvidenceHashResult,
) -> Result<String, EvidenceHashJobError> {
    let transaction = connection.unchecked_transaction()?;
    DataSourceRepo::new(&transaction).update_source_hash(
        data_source_id,
        Some(&result.digest),
        DataSourceHashStatus::Hashed,
    )?;
    let detail = format!(
        "Evidence SHA-256 complete: bytes={} segments={} workers={} backend={}",
        result.bytes_processed,
        result.parallel_segments,
        result.worker_threads,
        result.acceleration
    );
    JobRepo::new(&transaction).complete(job_id, &detail)?;
    transaction.commit()?;
    Ok(detail)
}

pub fn fail_hash_job(
    connection: &Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    detail: &str,
) -> Result<(), EvidenceHashJobError> {
    let transaction = connection.unchecked_transaction()?;
    let source_update = DataSourceRepo::new(&transaction).update_source_hash(
        data_source_id,
        None,
        DataSourceHashStatus::Failed,
    );
    JobRepo::new(&transaction).fail(job_id, detail)?;
    transaction.commit()?;
    if let Err(error) = source_update {
        tracing::warn!(%error, "Failed to mark evidence hash source as failed");
    }
    Ok(())
}

pub fn cancel_hash_job(
    connection: &Connection,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    detail: &str,
) -> Result<bool, EvidenceHashJobError> {
    let transaction = connection.unchecked_transaction()?;
    let source_update = DataSourceRepo::new(&transaction).update_source_hash(
        data_source_id,
        None,
        DataSourceHashStatus::Pending,
    );
    let changed = JobRepo::new(&transaction).cancel(job_id, detail)?;
    transaction.commit()?;
    if let Err(error) = source_update {
        tracing::warn!(%error, "Failed to return cancelled evidence hash to pending");
    }
    Ok(changed)
}

pub fn settle_registration_failure(
    connection: &Connection,
    job_id: &JobId,
    data_source_id: &DataSourceId,
    duplicate: bool,
) -> Result<(), EvidenceHashJobError> {
    let repo = JobRepo::new(connection);
    let prefix = detail_prefix(data_source_id);
    if duplicate {
        repo.cancel(job_id, &format!("{prefix}hash task already running"))?;
    } else {
        repo.fail(job_id, &format!("{prefix}task registration failed"))?;
    }
    Ok(())
}

fn load_source(
    connection: &Connection,
    data_source_id: &DataSourceId,
) -> Result<DataSource, EvidenceHashJobError> {
    let repo = DataSourceRepo::new(connection);
    let case_id = repo.case_id(data_source_id)?;
    repo.find_by_case(&case_id)?
        .into_iter()
        .find(|candidate| candidate.id == *data_source_id)
        .ok_or(EvidenceHashJobError::NotFound)
}

fn is_ready(
    repo: &DataSourceRepo<'_>,
    data_source_id: &DataSourceId,
) -> Result<bool, EvidenceHashJobError> {
    let Some(storage) = repo.find_storage(data_source_id)? else {
        return Ok(false);
    };
    Ok(matches!(
        storage.import_state.to_ascii_lowercase().as_str(),
        "ready" | "ready_metadata"
    ))
}

fn is_hash_candidate(source: &DataSource) -> bool {
    matches!(source.kind, DataSourceKind::E01 | DataSourceKind::Raw)
        && matches!(
            source.provenance.hash_status,
            DataSourceHashStatus::Pending | DataSourceHashStatus::Failed
        )
}

fn detail_prefix(data_source_id: &DataSourceId) -> String {
    format!("source={}; ", data_source_id.0)
}
