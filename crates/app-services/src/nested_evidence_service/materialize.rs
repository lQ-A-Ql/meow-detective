use std::{fs, io::Write, path::Path};

use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourceProvenance,
    DataSourceProvenanceStatus, FileEntryId, NestedEvidenceLineage,
};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
    nested_evidence_repo::NestedEvidenceRepo,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use thiserror::Error;
use transport::dto::{NestedEvidenceLineageDto, NestedEvidenceMaterializedDto};

use super::NestedEvidenceError;

const CHUNK_BYTES: u32 = 8 * 1024 * 1024;
const MAX_VHD_BYTES: u64 = 512 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum MaterializeError {
    #[error("nested evidence task cancelled")]
    Cancelled,
    #[error("nested evidence source is not ready: {0}")]
    Source(String),
    #[error("nested file is not a regular file")]
    NotFile,
    #[error("nested file exceeds the bounded materialization limit")]
    TooLarge,
    #[error("nested file is not a fixed VHD")]
    UnsupportedFormat,
    #[error("nested file materialization failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("derived evidence destination already exists")]
    Conflict,
    #[error("database error: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("database transaction failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Materializes a VHD found in a source-bound FileEntry. The parent source is
/// read only through its registered source database; no host path is resolved
/// from the catalog entry. The resulting file is an explicit derived source
/// under the case root and remains pending until the normal import pipeline
/// indexes it.
pub fn materialize_fixed_vhd(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_data_source_id: &str,
    file_entry_id: &str,
) -> Result<NestedEvidenceMaterializedDto, NestedEvidenceError> {
    materialize_fixed_vhd_with_cancel(
        case_conn,
        case_root,
        case_id,
        parent_data_source_id,
        file_entry_id,
        &std::sync::atomic::AtomicBool::new(false),
        &|_, _| {},
    )
}

pub fn materialize_fixed_vhd_with_cancel(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_data_source_id: &str,
    file_entry_id: &str,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<NestedEvidenceMaterializedDto, NestedEvidenceError> {
    materialize_inner(
        case_conn,
        case_root,
        case_id,
        parent_data_source_id,
        file_entry_id,
        cancel,
        progress,
    )
    .map_err(|error| match error {
        MaterializeError::Database(error) => NestedEvidenceError::Database(error),
        MaterializeError::Cancelled => NestedEvidenceError::Cancelled,
        other => NestedEvidenceError::Materialization(other),
    })
}

fn materialize_inner(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_id: &str,
    file_id: &str,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<NestedEvidenceMaterializedDto, MaterializeError> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        return Err(MaterializeError::Cancelled);
    }
    let parent = DataSourceId(parent_id.to_string());
    let (ready, entry, size) = load_entry(case_conn, case_root, case_id, &parent, file_id)?;
    let mut reader = crate::file_service::SourceReadContext::new(
        &ready.connection,
        case_conn,
        case_root,
        case_id,
        &parent,
    );
    let footer =
        crate::file_service::read_file_bytes_for_case(&mut reader, &entry.id, size - 512, 512)
            .map_err(|error| MaterializeError::Source(error.to_string()))?;
    validate_fixed_vhd_footer(&footer, size)?;

    let global_id = format!("ds:{parent_id}:{file_id}");
    let target = crate::digest_service::DigestTarget {
        data_source_id: Some(parent_id),
        file_id: Some(&global_id),
        partition_index: None,
    };
    let revision =
        crate::digest_service::identity::snapshot(case_conn, case_root, case_id, &target)
            .map_err(|error| MaterializeError::Source(error.to_string()))?
            .revision;
    let derived_id = DataSourceId(format!(
        "nested-{}",
        &hex::encode(Sha256::digest(format!("{parent_id}:{file_id}:{revision}")))[..32]
    ));
    if super::reuse::verify_existing(
        case_conn,
        case_root,
        case_id,
        &derived_id,
        size,
        cancel,
        progress,
    )? {
        return Ok(materialized_result(
            parent, entry.id.0, entry.path, derived_id, size,
        ));
    }
    let (output_path, temp_path) = prepare_output_paths(case_root, &derived_id)?;
    let write_result = copy_entry(
        &mut reader,
        &entry.id,
        size,
        &temp_path,
        &output_path,
        cancel,
        progress,
    );
    let hash = match write_result {
        Ok(hash) => hash,
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    };
    let unchanged =
        crate::digest_service::identity::snapshot(case_conn, case_root, case_id, &target)
            .map(|current| current.revision == revision)
            .unwrap_or(false);
    if !unchanged || cancel.load(std::sync::atomic::Ordering::Acquire) {
        let _ = fs::remove_file(&output_path);
        return Err(if unchanged {
            MaterializeError::Cancelled
        } else {
            MaterializeError::Source("parent evidence changed during materialization".into())
        });
    }
    let registration = register_materialized_vhd(
        case_conn,
        case_id,
        &parent,
        &entry,
        &derived_id,
        &output_path,
        &hash,
    );
    if let Err(error) = registration {
        let _ = fs::remove_file(&output_path);
        return Err(MaterializeError::Database(error));
    }
    Ok(materialized_result(
        parent, entry.id.0, entry.path, derived_id, size,
    ))
}

fn load_entry(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    parent: &DataSourceId,
    file_id: &str,
) -> Result<
    (
        crate::source_db::ReadySourceConnection,
        domain::FileEntry,
        u64,
    ),
    MaterializeError,
> {
    if DataSourceRepo::new(conn).case_id(parent)? != *case {
        return Err(MaterializeError::Source(
            "parent source is outside the active case".into(),
        ));
    }
    let ready = crate::source_db::open_ready_source_read_only_by_id(conn, root, case, parent)
        .map_err(|error| MaterializeError::Source(error.to_string()))?;
    let entry = FileRepo::new(&ready.connection)
        .find_by_id(&FileEntryId(file_id.into()))?
        .ok_or_else(|| MaterializeError::Source("nested file was not found".into()))?;
    if entry.data_source_id != *parent {
        return Err(MaterializeError::Source(
            "file does not belong to parent source".into(),
        ));
    }
    if !entry.is_file() {
        return Err(MaterializeError::NotFile);
    }
    let size = entry.size.ok_or(MaterializeError::UnsupportedFormat)?;
    if !(512..=MAX_VHD_BYTES).contains(&size) {
        return Err(MaterializeError::TooLarge);
    }
    Ok((ready, entry, size))
}

fn prepare_output_paths(
    case_root: &Path,
    derived_id: &DataSourceId,
) -> Result<(std::path::PathBuf, std::path::PathBuf), MaterializeError> {
    let output_dir = case_root.join("derived").join("nested-evidence");
    crate::source_db::safe_case_managed_destination(case_root, &output_dir)?;
    fs::create_dir_all(&output_dir)?;
    let canonical_root = case_root.canonicalize()?;
    let canonical_output_dir = output_dir.canonicalize()?;
    if !canonical_output_dir.starts_with(&canonical_root) {
        return Err(MaterializeError::Source(
            "derived evidence directory escapes the active case".into(),
        ));
    }
    let output_path = output_dir.join(format!("{}.vhd", derived_id.0));
    let temp_path = output_dir.join(format!("{}.tmp", derived_id.0));
    // These generated names identify unpublished files left by an interrupted
    // attempt. Registered sources are verified by reuse before reaching here.
    for path in [&output_path, &temp_path] {
        if path.try_exists()? {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(MaterializeError::Conflict);
            }
            fs::remove_file(path)?;
        }
    }
    Ok((output_path, temp_path))
}

fn register_materialized_vhd(
    case_conn: &Connection,
    case_id: &CaseId,
    parent: &DataSourceId,
    entry: &domain::FileEntry,
    derived_id: &DataSourceId,
    output_path: &Path,
    hash: &str,
) -> Result<(), persistence_sqlite::DbError> {
    let derived = DataSource {
        id: derived_id.clone(),
        name: format!("Nested VHD: {}", entry.path),
        kind: DataSourceKind::Raw,
        source_path: output_path.to_path_buf(),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance {
            source_hash_sha256: Some(hash.into()),
            hash_status: DataSourceHashStatus::Hashed,
            canonical_source_path: Some(output_path.to_path_buf()),
            evidence_size: entry.size,
            reader_kind: Some("vhd-fixed".into()),
            provenance_status: DataSourceProvenanceStatus::Recorded,
            warnings: vec!["derived source pending import pipeline".into()],
        },
    };
    let lineage = NestedEvidenceLineage {
        parent_data_source_id: parent.clone(),
        nested_file_path: entry.path.clone(),
        parent_file_id: Some(entry.id.0.clone()),
        derived_data_source_id: Some(derived_id.clone()),
        offset: 0,
        length: entry.size.unwrap_or_default(),
        probe_kind: "vhd-fixed".into(),
    };
    let transaction = case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    DataSourceRepo::new(&transaction).insert_with_storage(
        case_id,
        &derived,
        &DataSourceStorage::source_db(
            &derived_id.0,
            Some(super::platform::detect(output_path)?),
            Some("nested_vhd".into()),
        ),
    )?;
    NestedEvidenceRepo::new(&transaction).upsert(&lineage)?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)
}

fn materialized_result(
    parent: DataSourceId,
    parent_file_id: String,
    nested_file_path: String,
    derived_id: DataSourceId,
    size: u64,
) -> NestedEvidenceMaterializedDto {
    NestedEvidenceMaterializedDto {
        lineage: NestedEvidenceLineageDto {
            parent_data_source_id: parent.0,
            nested_file_path,
            parent_file_id: Some(parent_file_id),
            derived_data_source_id: Some(derived_id.0.clone()),
            offset: 0,
            length: size,
            probe_kind: "vhd-fixed".into(),
        },
        derived_data_source_id: derived_id.0,
        import_state: "pending".into(),
        import_job_id: None,
    }
}

fn validate_fixed_vhd_footer(footer: &[u8], size: u64) -> Result<(), MaterializeError> {
    if footer.len() != 512 || &footer[..8] != b"conectix" {
        return Err(MaterializeError::UnsupportedFormat);
    }
    let disk_type = u32::from_be_bytes(footer[60..64].try_into().expect("fixed slice"));
    let virtual_size = u64::from_be_bytes(footer[48..56].try_into().expect("fixed slice"));
    // A fixed VHD footer occupies the final 512 bytes and the virtual disk
    // payload must fit entirely before it.  Allowing `virtual_size == size`
    // would register a source whose logical reader exposes footer bytes.
    let payload_len = size.saturating_sub(512);
    let version = u32::from_be_bytes(footer[12..16].try_into().expect("fixed slice"));
    let checksum = u32::from_be_bytes(footer[64..68].try_into().expect("fixed slice"));
    let sum = footer
        .iter()
        .enumerate()
        .filter(|(i, _)| !(64..68).contains(i))
        .fold(0u32, |sum, (_, byte)| sum.wrapping_add(u32::from(*byte)));
    if disk_type != 2
        || version != 0x0001_0000
        || checksum != !sum
        || virtual_size == 0
        || virtual_size != payload_len
        || virtual_size % 512 != 0
    {
        return Err(MaterializeError::UnsupportedFormat);
    }
    Ok(())
}

fn copy_entry(
    reader: &mut crate::file_service::SourceReadContext<'_>,
    file_id: &FileEntryId,
    size: u64,
    temp_path: &Path,
    output_path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<String, MaterializeError> {
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)?;
    let mut offset = 0u64;
    let mut hash = Sha256::new();
    while offset < size {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Err(MaterializeError::Cancelled);
        }
        let amount = (size - offset).min(u64::from(CHUNK_BYTES)) as u32;
        let bytes =
            crate::file_service::read_file_bytes_for_case(&mut *reader, file_id, offset, amount)
                .map_err(|error| MaterializeError::Source(error.to_string()))?;
        if bytes.is_empty() || bytes.len() > amount as usize {
            return Err(MaterializeError::Source(
                "source returned an empty range".into(),
            ));
        }
        output.write_all(&bytes)?;
        hash.update(&bytes);
        offset += bytes.len() as u64;
        progress(offset, size);
    }
    output.sync_all()?;
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        return Err(MaterializeError::Cancelled);
    }
    drop(output);
    fs::hard_link(temp_path, output_path)?;
    fs::remove_file(temp_path)?;
    Ok(hex::encode(hash.finalize()))
}
