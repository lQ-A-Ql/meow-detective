use super::{DigestExecution, DigestServiceError, DigestTarget};
use crate::file_service::{self, SourceReadContext};
use crate::hash_service::{EvidenceHashError, HashService};
use domain::{CaseId, DataSourceKind};
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use transport::dto::{DigestAlgorithmDto, DigestScopeDto, DigestStatusDto, EvidenceDigestDto};

pub(super) fn execute_scope(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    scope: DigestScopeDto,
    algorithm: DigestAlgorithmDto,
    target: &DigestTarget<'_>,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let algorithm = map_algorithm(algorithm);
    match scope {
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
    }
}

fn digest_container_file(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: Option<&str>,
    algorithm: infrastructure::hashing::HashAlgorithm,
    execution: DigestExecution<'_>,
) -> Result<EvidenceDigestDto, DigestServiceError> {
    let source = super::load_source(case_conn, case_id, data_source_id)?;
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
    let source = super::load_source(case_conn, case_id, data_source_id)?;
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
    let source = super::load_source(case_conn, case_id, data_source_id)?;
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
    let source = super::load_source(case_conn, case_id, Some(source_id))?;
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
    let source = super::load_source(case_conn, case_id, data_source_id)?;
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
