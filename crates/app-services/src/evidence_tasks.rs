//! Case-bound persistent task planning and lifecycle, independent of the desktop runtime.

use domain::CaseId;
use persistence_sqlite::repositories::{
    evidence_task_repo::{EvidenceTaskRecord, EvidenceTaskRepo},
    job_repo::JobRepo,
};
use sha2::{Digest, Sha256};
use std::path::Path;
use transport::{
    dto::{EvidenceTaskDto, EvidenceTaskRequestDto, EvidenceTaskResultDto},
    CommandError,
};

pub struct EvidenceTaskPlan {
    pub task: EvidenceTaskDto,
    pub request: EvidenceTaskRequestDto,
    pub launch: bool,
}

pub fn plan(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
    request: EvidenceTaskRequestDto,
) -> Result<EvidenceTaskPlan, CommandError> {
    let kind = match &request {
        EvidenceTaskRequestDto::CarvePng(_) => "carvePng",
        EvidenceTaskRequestDto::CarveZip(_) => "carveZip",
        EvidenceTaskRequestDto::Digest(_) => "digest",
        EvidenceTaskRequestDto::NestedImport(_) => "nestedImport",
        EvidenceTaskRequestDto::ContentIndex { .. } => "contentIndex",
        EvidenceTaskRequestDto::RegistrySearch { .. } => "registrySearch",
    };
    let revision = revision(conn, root, case, &request)?;
    let json = serde_json::to_string(&request)
        .map_err(|_| CommandError::internal("Task request encoding failed"))?;
    let fingerprint = hex::encode(Sha256::digest(format!("{json}:{revision}")));
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)
        .map_err(CommandError::from_service_error)?;
    if let Some(record) = EvidenceTaskRepo::new(&tx)
        .reusable(&case.0, &fingerprint)
        .map_err(CommandError::from_service_error)?
    {
        if record.state != "completed" || kind == "digest" {
            return Ok(EvidenceTaskPlan {
                task: dto(record)?,
                request,
                launch: false,
            });
        }
    }
    if EvidenceTaskRepo::new(&tx)
        .active_count(&case.0)
        .map_err(CommandError::from_service_error)?
        >= 32
    {
        return Err(CommandError::conflict("Evidence task queue is full"));
    }
    let id = JobRepo::new(&tx)
        .create(&case.0, kind)
        .map_err(CommandError::from_service_error)?;
    let record = EvidenceTaskRecord {
        task_id: id.0,
        case_id: case.0.clone(),
        kind: kind.into(),
        state: "queued".into(),
        request_json: json,
        fingerprint,
        source_revision: revision,
        bytes_processed: 0,
        total_bytes: 0,
        result_json: None,
        error_code: None,
    };
    EvidenceTaskRepo::new(&tx)
        .insert(&record)
        .map_err(CommandError::from_service_error)?;
    tx.commit().map_err(CommandError::from_service_error)?;
    Ok(EvidenceTaskPlan {
        task: dto(record)?,
        request,
        launch: true,
    })
}

pub fn revision(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
    request: &EvidenceTaskRequestDto,
) -> Result<String, CommandError> {
    if let EvidenceTaskRequestDto::CarvePng(request) = request {
        crate::carving_service::validate_request(request)?;
        return crate::carving_service::source_revision(conn, root, case, &request.data_source_id);
    }
    if let EvidenceTaskRequestDto::CarveZip(request) = request {
        crate::carving_service::validate_zip_request(request)?;
        return crate::carving_service::source_revision_for(
            conn,
            root,
            case,
            &request.data_source_id,
            evidence_core::carving::ZIP_VALIDATOR_VERSION,
        );
    }
    if let EvidenceTaskRequestDto::RegistrySearch {
        file_id,
        session_id,
        query,
    } = request
    {
        if file_id.len() > 512
            || session_id.is_empty()
            || session_id.len() > 128
            || query.trim().is_empty()
            || query.len() > 256
        {
            return Err(CommandError::invalid_input("Invalid registry search"));
        }
        crate::file_service::open_source_for_file_id(conn, root, case, file_id)
            .map_err(CommandError::from_typed_service_error)?;
        return Ok(format!("registry-session:{session_id}"));
    }
    let nested_file = match request {
        EvidenceTaskRequestDto::NestedImport(request) => Some(format!(
            "ds:{}:{}",
            request.parent_data_source_id, request.file_entry_id
        )),
        _ => None,
    };
    let target = match request {
        EvidenceTaskRequestDto::Digest(request) => {
            request.validate().map_err(CommandError::invalid_input)?;
            crate::digest_service::DigestTarget {
                data_source_id: request.data_source_id.as_deref(),
                file_id: request.file_id.as_deref(),
                partition_index: request.partition_index,
            }
        }
        EvidenceTaskRequestDto::NestedImport(request) => {
            request.validate().map_err(CommandError::invalid_input)?;
            crate::digest_service::DigestTarget {
                data_source_id: Some(&request.parent_data_source_id),
                file_id: nested_file.as_deref(),
                partition_index: None,
            }
        }
        EvidenceTaskRequestDto::ContentIndex { data_source_id } => {
            if data_source_id.is_empty() || data_source_id.len() > 256 {
                return Err(CommandError::invalid_input("Invalid index source"));
            }
            crate::digest_service::DigestTarget {
                data_source_id: Some(data_source_id),
                file_id: None,
                partition_index: None,
            }
        }
        EvidenceTaskRequestDto::RegistrySearch { .. } => {
            return Err(CommandError::invalid_input("Invalid registry search"))
        }
        EvidenceTaskRequestDto::CarvePng(_) => {
            return Err(CommandError::invalid_input("Invalid carving scope"))
        }
        EvidenceTaskRequestDto::CarveZip(_) => {
            return Err(CommandError::invalid_input("Invalid carving scope"))
        }
    };
    Ok(
        crate::digest_service::identity::snapshot(conn, root, case, &target)
            .map_err(CommandError::from_typed_service_error)?
            .revision,
    )
}

pub fn get(
    conn: &rusqlite::Connection,
    case: &CaseId,
    task_id: &str,
) -> Result<EvidenceTaskDto, CommandError> {
    if task_id.len() > 128 {
        return Err(CommandError::invalid_input("Invalid task ID"));
    }
    let record = EvidenceTaskRepo::new(conn)
        .find(&case.0, task_id)
        .map_err(CommandError::from_service_error)?
        .ok_or_else(|| CommandError::not_found("Evidence task"))?;
    dto(record)
}

pub fn progress(
    conn: &rusqlite::Connection,
    case: &CaseId,
    task_id: &str,
    bytes: u64,
    total: u64,
) -> Result<u32, CommandError> {
    EvidenceTaskRepo::new(conn)
        .progress(&case.0, task_id, bytes, total)
        .map_err(CommandError::from_service_error)?;
    let percent = if total == 0 {
        0
    } else {
        ((bytes as u128 * 100 / total as u128).min(99)) as u32
    };
    JobRepo::new(conn)
        .update_progress(
            &domain::JobId(task_id.into()),
            percent,
            "Processing evidence",
        )
        .map_err(CommandError::from_service_error)?;
    Ok(percent)
}

pub fn finish(
    conn: &rusqlite::Connection,
    case: &CaseId,
    task_id: &str,
    result: Result<EvidenceTaskResultDto, CommandError>,
) -> Result<(), CommandError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(CommandError::from_service_error)?;
    let repo = EvidenceTaskRepo::new(&tx);
    let job = JobRepo::new(&tx);
    let id = domain::JobId(task_id.into());
    match result {
        Ok(result) => {
            let total = match &result {
                EvidenceTaskResultDto::CarvePng(summary) => {
                    summary.scanned_bytes.parse::<u64>().unwrap_or(0)
                }
                EvidenceTaskResultDto::CarveZip(summary) => {
                    summary.scanned_bytes.parse::<u64>().unwrap_or(0)
                }
                EvidenceTaskResultDto::Digest(digest) => digest.byte_length,
                EvidenceTaskResultDto::NestedImport(nested) => nested.lineage.length,
                EvidenceTaskResultDto::ContentIndex(index) => index.expected,
                EvidenceTaskResultDto::RegistrySearch(page) => {
                    u64::from(page.scanned_keys) + u64::from(page.scanned_values)
                }
            };
            repo.progress(&case.0, task_id, total, total)
                .map_err(CommandError::from_service_error)?;
            let json = serde_json::to_string(&result)
                .map_err(|_| CommandError::internal("Task result encoding failed"))?;
            repo.finish(&case.0, task_id, "completed", Some(&json), None)
                .map_err(CommandError::from_service_error)?;
            job.complete_committed(&id, "Evidence task completed")
                .map_err(CommandError::from_service_error)?;
        }
        Err(error) => {
            let cancelled = error.category == "cancelled";
            repo.finish(
                &case.0,
                task_id,
                if cancelled { "cancelled" } else { "failed" },
                None,
                Some(&error.code),
            )
            .map_err(CommandError::from_service_error)?;
            if cancelled {
                job.cancel(&id, "Evidence task cancelled").map(|_| ())
            } else {
                job.fail(&id, "Evidence task failed")
            }
            .map_err(CommandError::from_service_error)?;
        }
    }
    tx.commit().map_err(CommandError::from_service_error)?;
    Ok(())
}

fn dto(record: EvidenceTaskRecord) -> Result<EvidenceTaskDto, CommandError> {
    let result = record
        .result_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| CommandError::internal("Stored task result is corrupt"))?;
    Ok(EvidenceTaskDto {
        task_id: record.task_id,
        kind: record.kind,
        state: record.state,
        source_revision: record.source_revision,
        bytes_processed: record.bytes_processed,
        total_bytes: record.total_bytes,
        result,
        error_code: record.error_code,
    })
}
