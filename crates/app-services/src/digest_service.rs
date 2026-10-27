//! Case-scoped evidence digest use case.
use crate::file_service::{self, SourceReadContext};
use crate::hash_service::{EvidenceHashError, HashService};
use domain::{CaseId, DataSourceId, DataSourceKind};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::repositories::evidence_digest_repo::{
    EvidenceDigestRecord, EvidenceDigestRepo,
};
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use transport::dto::{DigestAlgorithmDto, DigestScopeDto, DigestStatusDto, EvidenceDigestDto};
use transport::{ErrorCategory, ServiceErrorCategory};
pub(crate) mod identity;
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
            Self::Hash(EvidenceHashError::Unsupported) | Self::Unsupported => {
                ErrorCategory::Unsupported
            }
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
    let algorithm = map_algorithm(algorithm);
    let result = match scope {
        DigestScopeDto::ContainerFile => digest_container_file(
            case_conn,
            case_id,
            target.data_source_id,
            algorithm,
            execution,
        ),
        DigestScopeDto::ContainerSet => digest_container_set(
            case_conn,
            case_id,
            target.data_source_id,
            algorithm,
            execution,
        ),
        DigestScopeDto::File => {
            let file_id = target.file_id.ok_or(DigestServiceError::InvalidInput)?;
            digest_file(case_conn, case_root, case_id, file_id, algorithm, execution)
        }
        DigestScopeDto::LogicalDisk => digest_logical_disk(
            case_conn,
            case_id,
            target.data_source_id,
            algorithm,
            execution,
        ),
        DigestScopeDto::Partition => digest_partition(
            case_conn,
            case_id,
            target.data_source_id,
            target.partition_index,
            algorithm,
            execution,
        ),
        DigestScopeDto::DerivedEvidence => digest_derived(
            case_conn,
            case_id,
            target.data_source_id,
            algorithm,
            execution,
        ),
    };
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
fn digest_container_file(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: Option<&str>,
    algorithm: infrastructure::hashing::HashAlgorithm,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let source = load_source(case_conn, case_id, data_source_id)?;
    let mut file = std::fs::File::open(&source.source_path).map_err(|error| {
        DigestServiceError::Hash(EvidenceHashError::Io {
            operation: "open container",
            kind: error.kind(),
        })
    })?;
    let total = file
        .metadata()
        .map_err(|error| {
            DigestServiceError::Hash(EvidenceHashError::Io {
                operation: "inspect container",
                kind: error.kind(),
            })
        })?
        .len();
    let processed = AtomicU64::new(0);
    let digest = HashService::digest_reader(
        &mut file,
        algorithm,
        || execution.cancelled.load(Ordering::Acquire),
        |bytes| {
            processed.store(bytes, Ordering::Release);
            (execution.progress)(bytes, total);
        },
    )
    .map_err(|error| {
        DigestServiceError::Hash(EvidenceHashError::Io {
            operation: "read container file",
            kind: error.kind(),
        })
    })?
    .ok_or(DigestServiceError::Hash(EvidenceHashError::Cancelled))?;
    if processed.load(Ordering::Acquire) != total {
        return Err(short_read());
    }
    let byte_length = std::fs::metadata(&source.source_path)
        .map_err(|error| {
            DigestServiceError::Hash(EvidenceHashError::Io {
                operation: "inspect container file",
                kind: error.kind(),
            })
        })?
        .len();
    Ok(completed(
        DigestScopeDto::ContainerFile,
        algorithm,
        digest,
        byte_length,
    ))
}
fn digest_container_set(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: Option<&str>,
    algorithm: infrastructure::hashing::HashAlgorithm,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let source = load_source(case_conn, case_id, data_source_id)?;
    let result = HashService::hash_evidence_with_algorithm(
        Path::new(&source.source_path),
        &source.kind,
        algorithm,
        execution.cancelled,
        execution.progress,
    )
    .map_err(DigestServiceError::Hash)?;
    Ok(completed(
        DigestScopeDto::ContainerSet,
        algorithm,
        result.digest,
        result.bytes_processed,
    ))
}
fn digest_logical_disk(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: Option<&str>,
    algorithm: infrastructure::hashing::HashAlgorithm,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let source = load_source(case_conn, case_id, data_source_id)?;
    let result = HashService::hash_logical_evidence(
        Path::new(&source.source_path),
        &source.kind,
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
fn digest_partition(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: Option<&str>,
    partition_index: Option<u32>,
    algorithm: infrastructure::hashing::HashAlgorithm,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let source_id = data_source_id.ok_or(DigestServiceError::InvalidInput)?;
    let index = partition_index.ok_or(DigestServiceError::InvalidInput)? as usize;
    let source = load_source(case_conn, case_id, Some(source_id))?;
    let partition = persistence_sqlite::repositories::partition_repo::PartitionRepo::new(case_conn)
        .find_by_data_source_and_index(source_id, index)?
        .ok_or(DigestServiceError::NotFound)?;
    let reader = crate::datasource_service::open_evidence_reader(&source.source_path, &source.kind)
        .map_err(|error| {
            DigestServiceError::Hash(EvidenceHashError::Io {
                operation: "open partition reader",
                kind: error.kind(),
            })
        })?;
    let mut window =
        evidence_core::PartitionWindowReader::new(reader, partition.offset, Some(partition.length))
            .map_err(|error| {
                DigestServiceError::Hash(EvidenceHashError::Io {
                    operation: "open partition window",
                    kind: error.kind(),
                })
            })?;
    let processed = AtomicU64::new(0);
    let digest = HashService::digest_reader(
        &mut window,
        algorithm,
        || execution.cancelled.load(Ordering::Acquire),
        |amount| {
            processed.store(amount, Ordering::Release);
            (execution.progress)(amount, partition.length);
        },
    )
    .map_err(|error| {
        DigestServiceError::Hash(EvidenceHashError::Io {
            operation: "read partition",
            kind: error.kind(),
        })
    })?
    .ok_or(DigestServiceError::Hash(EvidenceHashError::Cancelled))?;
    if processed.load(Ordering::Acquire) != partition.length {
        return Err(short_read());
    }
    Ok(completed(
        DigestScopeDto::Partition,
        algorithm,
        digest,
        partition.length,
    ))
}
fn digest_derived(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: Option<&str>,
    algorithm: infrastructure::hashing::HashAlgorithm,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let source = load_source(case_conn, case_id, data_source_id)?;
    let result = match &source.kind {
        DataSourceKind::Raw | DataSourceKind::AndroidSparse | DataSourceKind::LocalDisk => {
            HashService::hash_logical_evidence(
                Path::new(&source.source_path),
                &source.kind,
                algorithm,
                execution.cancelled,
                execution.progress,
            )
        }
        _ => HashService::hash_evidence_with_algorithm(
            Path::new(&source.source_path),
            &source.kind,
            algorithm,
            execution.cancelled,
            execution.progress,
        ),
    }
    .map_err(DigestServiceError::Hash)?;
    Ok(completed(
        DigestScopeDto::DerivedEvidence,
        algorithm,
        result.digest,
        result.bytes_processed,
    ))
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
    let total = persistence_sqlite::repositories::file_repo::FileRepo::new(&source_conn)
        .find_by_id(&global_id.local_id)?
        .ok_or(DigestServiceError::NotFound)?
        .size;
    let mut reader = file_service::open_file_content_by_id(&mut context, &global_id.local_id)?;
    let processed = AtomicU64::new(0);
    let digest = HashService::digest_reader(
        reader.as_mut(),
        algorithm,
        || execution.cancelled.load(Ordering::Acquire),
        |amount| {
            processed.store(amount, Ordering::Release);
            (execution.progress)(amount, total.unwrap_or_default());
        },
    )
    .map_err(|error| {
        DigestServiceError::Hash(EvidenceHashError::Io {
            operation: "read logical file",
            kind: error.kind(),
        })
    })?
    .ok_or(DigestServiceError::Hash(EvidenceHashError::Cancelled))?;
    if total.is_some_and(|total| total != processed.load(Ordering::Acquire)) {
        return Err(short_read());
    }
    Ok(completed(
        DigestScopeDto::File,
        algorithm,
        digest,
        processed.load(Ordering::Acquire),
    ))
}
fn short_read() -> DigestServiceError {
    DigestServiceError::Hash(EvidenceHashError::Io {
        operation: "read complete digest input",
        kind: std::io::ErrorKind::UnexpectedEof,
    })
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
