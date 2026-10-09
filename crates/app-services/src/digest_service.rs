//! Case-scoped evidence digest use case.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use domain::{CaseId, DataSourceId, DataSourceKind};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use transport::dto::{DigestAlgorithmDto, DigestScopeDto, DigestStatusDto, EvidenceDigestDto};
use transport::{ErrorCategory, ServiceErrorCategory};

use crate::file_service::{self, SourceReadContext};
use crate::hash_service::{EvidenceHashError, HashService};

#[derive(Debug, thiserror::Error)]
pub enum DigestServiceError {
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
    fn category(&self) -> ErrorCategory {
        match self {
            Self::Database(_) | Self::File(file_service::FileServiceError::Io(_)) => {
                ErrorCategory::Io
            }
            Self::File(error) => error.category(),
            Self::Hash(EvidenceHashError::Io { .. }) => ErrorCategory::Io,
            Self::Hash(EvidenceHashError::Cancelled) => ErrorCategory::Cancelled,
            Self::Hash(EvidenceHashError::Unsupported) | Self::Unsupported => {
                ErrorCategory::Unsupported
            }
            Self::NotFound | Self::InvalidInput => ErrorCategory::Validation,
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
/// observe byte progress.  The `LogicalDisk` scope is intentionally restricted
/// to `DataSourceKind::LocalDisk`; image containers must use `ContainerSet` so
/// their container bytes are never confused with physical-disk bytes.
pub fn calculate_evidence_digest_with_cancel(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    scope: DigestScopeDto,
    algorithm: DigestAlgorithmDto,
    target: DigestTarget<'_>,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let algorithm = map_algorithm(algorithm);
    match scope {
        DigestScopeDto::ContainerFile => {
            let source = load_source(case_conn, case_id, target.data_source_id)?;
            let digest = HashService::digest_file(Path::new(&source.source_path), algorithm)
                .map_err(|error| {
                    DigestServiceError::Hash(EvidenceHashError::Io {
                        operation: "read container file",
                        kind: error.kind(),
                    })
                })?;
            let byte_length = std::fs::metadata(&source.source_path)
                .map_err(|error| {
                    DigestServiceError::Hash(EvidenceHashError::Io {
                        operation: "inspect container file",
                        kind: error.kind(),
                    })
                })?
                .len();
            Ok(completed(scope, algorithm, digest, byte_length))
        }
        DigestScopeDto::ContainerSet => {
            let source = load_source(case_conn, case_id, target.data_source_id)?;
            let result = HashService::hash_evidence_with_algorithm(
                Path::new(&source.source_path),
                &source.kind,
                algorithm,
                execution.cancelled,
                execution.progress,
            )
            .map_err(DigestServiceError::Hash)?;
            Ok(completed(
                scope,
                algorithm,
                result.digest,
                result.bytes_processed,
            ))
        }
        DigestScopeDto::File => {
            let file_id = target.file_id.ok_or(DigestServiceError::InvalidInput)?;
            digest_file(case_conn, case_root, case_id, file_id, algorithm, execution)
        }
        DigestScopeDto::LogicalDisk => {
            let source = load_source(case_conn, case_id, target.data_source_id)?;
            if source.kind != DataSourceKind::LocalDisk {
                return Err(DigestServiceError::Unsupported);
            }
            let result = HashService::hash_evidence_with_algorithm(
                Path::new(&source.source_path),
                &DataSourceKind::LocalDisk,
                algorithm,
                execution.cancelled,
                execution.progress,
            )
            .map_err(DigestServiceError::Hash)?;
            Ok(completed(
                DigestScopeDto::LogicalDisk,
                algorithm,
                result.digest,
                result.bytes_processed,
            ))
        }
        DigestScopeDto::Partition | DigestScopeDto::DerivedEvidence => {
            let _ = (target.data_source_id, target.partition_index);
            Err(DigestServiceError::Unsupported)
        }
    }
}

fn digest_file(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    algorithm: infrastructure::hashing::HashAlgorithm,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let (global_id, source_conn) =
        file_service::open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let mut context = SourceReadContext::new(
        &source_conn,
        case_conn,
        case_root,
        case_id,
        &global_id.data_source_id,
    );
    let mut reader = file_service::open_file_content_by_id(&mut context, &global_id.local_id)?;
    let processed = AtomicU64::new(0);
    let digest = HashService::digest_reader(
        reader.as_mut(),
        algorithm,
        || execution.cancelled.load(Ordering::Acquire),
        |amount| {
            processed.store(amount, Ordering::Release);
            (execution.progress)(amount, 0);
        },
    )
    .map_err(|error| {
        DigestServiceError::Hash(EvidenceHashError::Io {
            operation: "read logical file",
            kind: error.kind(),
        })
    })?
    .ok_or(DigestServiceError::Hash(EvidenceHashError::Cancelled))?;
    Ok(completed(
        DigestScopeDto::File,
        algorithm,
        digest,
        processed.load(Ordering::Acquire),
    ))
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

fn map_algorithm(algorithm: DigestAlgorithmDto) -> infrastructure::hashing::HashAlgorithm {
    match algorithm {
        DigestAlgorithmDto::Md5 => infrastructure::hashing::HashAlgorithm::Md5,
        DigestAlgorithmDto::Sha1 => infrastructure::hashing::HashAlgorithm::Sha1,
        DigestAlgorithmDto::Sha256 => infrastructure::hashing::HashAlgorithm::Sha256,
        DigestAlgorithmDto::Sm3 => infrastructure::hashing::HashAlgorithm::Sm3,
    }
}

fn completed(
    scope: DigestScopeDto,
    algorithm: infrastructure::hashing::HashAlgorithm,
    value: String,
    byte_length: u64,
) -> EvidenceDigestDto {
    EvidenceDigestDto {
        scope,
        algorithm: match algorithm {
            infrastructure::hashing::HashAlgorithm::Md5 => DigestAlgorithmDto::Md5,
            infrastructure::hashing::HashAlgorithm::Sha1 => DigestAlgorithmDto::Sha1,
            infrastructure::hashing::HashAlgorithm::Sha256 => DigestAlgorithmDto::Sha256,
            infrastructure::hashing::HashAlgorithm::Sm3 => DigestAlgorithmDto::Sm3,
        },
        value,
        byte_length,
        status: DigestStatusDto::Completed,
    }
}

#[cfg(test)]
#[path = "../tests/unit/digest_service.rs"]
mod tests;
