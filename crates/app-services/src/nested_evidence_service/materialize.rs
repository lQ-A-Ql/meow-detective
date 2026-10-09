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
use thiserror::Error;
use transport::dto::{NestedEvidenceLineageDto, NestedEvidenceMaterializedDto};

use super::NestedEvidenceError;

const CHUNK_BYTES: u32 = 8 * 1024 * 1024;
const MAX_VHD_BYTES: u64 = 512 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum MaterializeError {
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
    materialize_inner(
        case_conn,
        case_root,
        case_id,
        parent_data_source_id,
        file_entry_id,
    )
    .map_err(|error| match error {
        MaterializeError::Database(error) => NestedEvidenceError::Database(error),
        other => NestedEvidenceError::Materialization(other.to_string()),
    })
}

fn materialize_inner(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_id: &str,
    file_id: &str,
) -> Result<NestedEvidenceMaterializedDto, MaterializeError> {
    let parent = DataSourceId(parent_id.to_string());
    let owner = DataSourceRepo::new(case_conn).case_id(&parent)?;
    if owner != *case_id {
        return Err(MaterializeError::Source(
            "parent source is outside the active case".into(),
        ));
    }
    let ready =
        crate::source_db::open_ready_source_read_only_by_id(case_conn, case_root, case_id, &parent)
            .map_err(|error| MaterializeError::Source(error.to_string()))?;
    let entry = FileRepo::new(&ready.connection)
        .find_by_id(&FileEntryId(file_id.to_string()))?
        .ok_or_else(|| MaterializeError::Source("nested file was not found".into()))?;
    if entry.data_source_id != parent {
        return Err(MaterializeError::Source(
            "file does not belong to parent source".into(),
        ));
    }
    if entry.entry_type != domain::EntryType::File {
        return Err(MaterializeError::NotFile);
    }
    let size = entry.size.ok_or(MaterializeError::UnsupportedFormat)?;
    if size < 512 || size > MAX_VHD_BYTES {
        return Err(MaterializeError::TooLarge);
    }
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

    let derived_id = DataSourceId(format!("nested-{}", uuid::Uuid::new_v4()));
    let output_dir = case_root.join("derived").join("nested-evidence");
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
    if output_path.exists() || temp_path.exists() {
        return Err(MaterializeError::Conflict);
    }
    let write_result = copy_entry(&mut reader, &entry.id, size, &temp_path, &output_path);
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return write_result.map(|_| unreachable!());
    }
    let derived = DataSource {
        id: derived_id.clone(),
        name: format!("Nested VHD: {}", entry.path),
        kind: DataSourceKind::Raw,
        source_path: output_path.clone(),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance {
            source_hash_sha256: None,
            hash_status: DataSourceHashStatus::Pending,
            canonical_source_path: Some(output_path.clone()),
            evidence_size: Some(size),
            reader_kind: Some("vhd-fixed".into()),
            provenance_status: DataSourceProvenanceStatus::Recorded,
            warnings: vec!["derived source pending import pipeline".into()],
        },
    };
    // Register the source before publishing lineage.  Both writes are
    // idempotent at the database layer only when performed as one transaction;
    // callers retrying after a crash must never observe a lineage pointing to
    // a missing data source.
    let lineage = NestedEvidenceLineage {
        parent_data_source_id: parent,
        nested_file_path: entry.path,
        derived_data_source_id: Some(derived_id.clone()),
        offset: 0,
        length: size,
        probe_kind: "vhd-fixed".into(),
    };
    let registration = (|| -> Result<(), persistence_sqlite::DbError> {
        let transaction = case_conn
            .unchecked_transaction()
            .map_err(persistence_sqlite::DbError::from)?;
        DataSourceRepo::new(&transaction).insert_with_storage(
            case_id,
            &derived,
            &DataSourceStorage::source_db(
                &derived_id.0,
                Some("windows"),
                Some("nested_vhd".into()),
            ),
        )?;
        NestedEvidenceRepo::new(&transaction).upsert(&lineage)?;
        transaction
            .commit()
            .map_err(persistence_sqlite::DbError::from)
    })();
    if let Err(error) = registration {
        let _ = fs::remove_file(&output_path);
        return Err(MaterializeError::Database(error));
    }
    Ok(NestedEvidenceMaterializedDto {
        lineage: NestedEvidenceLineageDto {
            parent_data_source_id: lineage.parent_data_source_id.0,
            nested_file_path: lineage.nested_file_path,
            derived_data_source_id: Some(derived_id.0.clone()),
            offset: 0,
            length: size,
            probe_kind: "vhd-fixed".into(),
        },
        derived_data_source_id: derived_id.0,
        import_state: "pending".into(),
    })
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
    let payload_len = size.checked_sub(512).unwrap_or_default();
    if disk_type != 2 || virtual_size == 0 || virtual_size > payload_len {
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
) -> Result<(), MaterializeError> {
    let mut output = fs::File::create(temp_path)?;
    let mut offset = 0u64;
    while offset < size {
        let amount = (size - offset).min(u64::from(CHUNK_BYTES)) as u32;
        let bytes =
            crate::file_service::read_file_bytes_for_case(&mut *reader, file_id, offset, amount)
                .map_err(|error| MaterializeError::Source(error.to_string()))?;
        if bytes.is_empty() {
            return Err(MaterializeError::Source(
                "source returned an empty range".into(),
            ));
        }
        output.write_all(&bytes)?;
        offset = offset.saturating_add(bytes.len() as u64);
    }
    output.sync_all()?;
    fs::rename(temp_path, output_path)?;
    Ok(())
}
