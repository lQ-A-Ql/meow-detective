//! Case-scoped evidence digest use case.
use crate::file_service;
use crate::hash_service::EvidenceHashError;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::repositories::evidence_digest_repo::{
    EvidenceDigestRecord, EvidenceDigestRepo,
};
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use transport::dto::{DigestAlgorithmDto, DigestScopeDto, EvidenceDigestDto};
use transport::{ErrorCategory, ServiceErrorCategory};
mod executor;
pub mod identity;
pub mod results;
#[derive(Debug, thiserror::Error)]
pub enum DigestServiceError {
    #[error("evidence changed during digest calculation")]
    EvidenceChanged,
    #[error("digest database operation failed")]
    Database(#[source] persistence_sqlite::DbError),
    #[error("digest target was not found")]
    NotFound,
    #[error("digest scope is not supported by the active evidence reader")]
    Unsupported,
    #[error("digest target is invalid")]
    InvalidInput,
    #[error("digest file read failed")]
    File(#[source] file_service::FileServiceError),
    #[error("digest computation failed")]
    Hash(#[source] EvidenceHashError),
}
pub struct DigestTarget<'a> {
    pub data_source_id: Option<&'a str>,
    pub file_id: Option<&'a str>,
    pub partition_index: Option<u32>,
}
#[derive(Clone, Copy)]
pub struct DigestExecution<'a> {
    pub cancelled: &'a AtomicBool,
    pub progress: &'a (dyn Fn(u64, u64) + Sync),
}
impl From<persistence_sqlite::DbError> for DigestServiceError {
    fn from(error: persistence_sqlite::DbError) -> Self {
        Self::Database(error)
    }
}
impl From<file_service::FileServiceError> for DigestServiceError {
    fn from(error: file_service::FileServiceError) -> Self {
        Self::File(error)
    }
}
impl ServiceErrorCategory for DigestServiceError {
    fn code(&self) -> Option<&'static str> {
        match self {
            Self::EvidenceChanged => Some("evidence_changed"),
            _ => None,
        }
    }
    fn user_message(&self) -> Option<&'static str> {
        match self {
            Self::EvidenceChanged => {
                Some("Evidence changed during calculation; retry against the current revision")
            }
            _ => Some("Evidence digest operation failed"),
        }
    }
    fn category(&self) -> ErrorCategory {
        match self {
            Self::Database(_) | Self::File(file_service::FileServiceError::Io(_)) => {
                ErrorCategory::Io
            }
            Self::File(error) => error.category(),
            Self::Hash(EvidenceHashError::Io { .. }) => ErrorCategory::Io,
            Self::Hash(EvidenceHashError::Cancelled) => ErrorCategory::Cancelled,
            Self::Hash(EvidenceHashError::Unsupported)
            | Self::Hash(EvidenceHashError::DirectoryEntryLimit)
            | Self::Unsupported => ErrorCategory::Unsupported,
            Self::NotFound | Self::InvalidInput => ErrorCategory::Validation,
            Self::EvidenceChanged => ErrorCategory::Security,
        }
    }
}
/// Calculates one digest against an active case without exposing a host path.
///
/// The command layer runs this use case on a blocking worker. Scope support is
/// deliberately explicit: unsupported scopes return a typed error rather than
/// falling back to a potentially misleading source hash.
pub fn calculate_evidence_digest(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    scope: DigestScopeDto,
    algorithm: DigestAlgorithmDto,
    target: DigestTarget<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let cancelled = AtomicBool::new(false);
    calculate_evidence_digest_with_cancel(
        case_conn,
        case_root,
        case_id,
        scope,
        algorithm,
        target,
        DigestExecution {
            cancelled: &cancelled,
            progress: &|_, _| {},
        },
    )
}
/// Calculates a digest while allowing a task manager to cancel the read and
/// observe byte progress. The `LogicalDisk` scope hashes the validated logical
/// reader bytes; container bytes remain available through `ContainerSet`.
pub fn calculate_evidence_digest_with_cancel(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    scope: DigestScopeDto,
    algorithm: DigestAlgorithmDto,
    target: DigestTarget<'_>,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let identity = identity::snapshot(case_conn, case_root, case_id, &target)?;
    let result = executor::execute_scope(
        case_conn, case_root, case_id, scope, algorithm, &target, execution,
    );
    if execution.cancelled.load(Ordering::Acquire) {
        return Err(DigestServiceError::Hash(EvidenceHashError::Cancelled));
    }
    let after = identity::snapshot(case_conn, case_root, case_id, &target);
    if after
        .as_ref()
        .is_ok_and(|after| after.revision != identity.revision)
    {
        return Err(DigestServiceError::EvidenceChanged);
    }
    if let Ok(digest) = &result {
        after?;
        if scope == DigestScopeDto::File && algorithm == DigestAlgorithmDto::Sha256 {
            if let Some(file_id) = target.file_id {
                let (global, source_conn) =
                    file_service::open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
                persistence_sqlite::repositories::file_repo::FileRepo::new(&source_conn)
                    .update_hash_sha256(&global.local_id, &digest.value)?
                    .ok_or(DigestServiceError::NotFound)?;
            }
        }
        EvidenceDigestRepo::new(case_conn).upsert(&EvidenceDigestRecord {
            case_id: case_id.0.clone(),
            scope: format!("{:?}", digest.scope),
            algorithm: digest.algorithm.label().to_string(),
            data_source_id: Some(identity.source_id),
            file_id: identity.file_id,
            partition_index: target.partition_index,
            value: digest.value.clone(),
            byte_length: digest.byte_length,
            status: "completed".into(),
            source_revision: identity.revision,
        })?;
    }
    result
}

pub fn current_logical_disk_sha256(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &str,
) -> Result<Option<String>, DigestServiceError> {
    let target = DigestTarget {
        data_source_id: Some(data_source_id),
        file_id: None,
        partition_index: None,
    };
    let identity = identity::snapshot(case_conn, case_root, case_id, &target)?;
    let record = EvidenceDigestRepo::new(case_conn)
        .list(case_id.0.as_str(), data_source_id, None, None)?
        .into_iter()
        .find(|record| {
            record.scope == "LogicalDisk"
                && record.algorithm == "sha256"
                && record.status == "completed"
        });
    Ok(record
        .and_then(|record| (record.source_revision == identity.revision).then_some(record.value)))
}

fn load_source(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: Option<&str>,
) -> Result<domain::DataSource, DigestServiceError> {
    let id = DataSourceId(
        data_source_id
            .filter(|value| !value.trim().is_empty())
            .ok_or(DigestServiceError::InvalidInput)?
            .to_string(),
    );
    DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == id)
        .ok_or(DigestServiceError::NotFound)
}
#[cfg(test)]
#[path = "../tests/unit/digest_service.rs"]
mod tests;
