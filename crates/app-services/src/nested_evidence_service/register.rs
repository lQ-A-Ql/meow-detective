use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, FileEntryId, NestedEvidenceLineage,
};
use evidence_access::source_runtime::nested_range::NestedRangeFactory;
use evidence_core::EvidenceReaderFactory;
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    nested_evidence_repo::{NestedEvidenceRepo, NestedRangeLocator},
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use transport::dto::NestedEvidenceMaterializedDto;

use super::{NestedEvidenceError, NestedEvidenceMaterialized};

pub fn register_fixed_vhd_range(
    conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_id: &str,
    file_id: &str,
    revision: &str,
    cancel: &AtomicBool,
) -> Result<NestedEvidenceMaterialized, NestedEvidenceError> {
    check_cancel(cancel)?;
    let parent_id = DataSourceId(parent_id.into());
    let parent =
        crate::source_db::open_ready_source_read_only_by_id(conn, case_root, case_id, &parent_id)
            .map_err(std::io::Error::other)?;
    let entry = FileRepo::new(&parent.connection)
        .find_by_id(&FileEntryId(file_id.into()))?
        .filter(|entry| entry.is_file() && entry.data_source_id == parent_id)
        .ok_or(NestedEvidenceError::UnsupportedFormat)?;
    let length = entry
        .size
        .and_then(|size| size.checked_sub(512))
        .filter(|length| *length > 0 && *length <= 512 * 1024 * 1024 * 1024 && *length % 512 == 0)
        .ok_or(NestedEvidenceError::UnsupportedFormat)?;
    let locator = NestedRangeLocator {
        parent_id,
        file_id: entry.id,
        revision: revision.into(),
        length,
    };
    let factory = NestedRangeFactory::from_locator(case_root, case_id, &locator);
    let source_id = DataSourceId(format!(
        "nested-range-{}",
        &hex::encode(Sha256::digest(format!(
            "{}:{file_id}:{revision}",
            locator.parent_id.0
        )))[..32]
    ));
    let lineage = NestedEvidenceLineage {
        parent_data_source_id: locator.parent_id.clone(),
        nested_file_path: entry.path,
        parent_file_id: Some(locator.file_id.0.clone()),
        derived_data_source_id: Some(source_id.clone()),
        offset: 0,
        length,
        probe_kind: "vhd-fixed-range".into(),
    };
    let repo = NestedEvidenceRepo::new(conn);
    let probe = if let Some(existing) = repo.find_range(&source_id)? {
        if existing != locator {
            return Err(NestedEvidenceError::WrongCase);
        }
        factory.validate_snapshot()?;
        None
    } else {
        let mut reader = factory.open_reader()?;
        let detected = super::platform::detect(reader.as_mut())?;
        factory.validate_snapshot()?;
        check_cancel(cancel)?;
        persist_range(
            conn,
            case_id,
            &source_id,
            &locator,
            &lineage,
            detected.platform,
        )?;
        Some(detected.probe)
    };
    Ok(NestedEvidenceMaterialized {
        result: NestedEvidenceMaterializedDto {
            lineage: super::to_dto(lineage),
            derived_data_source_id: source_id.0,
            import_state: "pending".into(),
            import_job_id: None,
        },
        probe,
    })
}

fn persist_range(
    conn: &Connection,
    case_id: &CaseId,
    id: &DataSourceId,
    locator: &NestedRangeLocator,
    lineage: &NestedEvidenceLineage,
    platform: &str,
) -> Result<(), NestedEvidenceError> {
    let mut provenance = domain::DataSourceProvenance::unknown();
    provenance.evidence_size = Some(locator.length);
    provenance.reader_kind = Some("vhd-fixed-range".into());
    let source = DataSource {
        id: id.clone(),
        name: format!("Nested VHD: {}", lineage.nested_file_path),
        kind: DataSourceKind::NestedRange,
        source_path: id.0.clone().into(),
        imported_at: chrono::Utc::now(),
        provenance,
    };
    let tx = conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    let storage = DataSourceStorage::source_db(&id.0, Some(platform), Some("nested_range".into()));
    DataSourceRepo::new(&tx).insert_with_storage(case_id, &source, &storage)?;
    let repo = NestedEvidenceRepo::new(&tx);
    repo.insert_range(id, locator)?;
    repo.upsert(lineage)?;
    tx.commit().map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), NestedEvidenceError> {
    if cancel.load(Ordering::Acquire) {
        return Err(NestedEvidenceError::Cancelled);
    }
    Ok(())
}
