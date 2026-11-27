use chrono::Utc;
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use evidence_access::lvm::{
    lvm_volume_fingerprint, open_lvm_volume_reader, read_lvm_descriptor, write_lvm_descriptor,
    LvmPlanError, LvmVolumePlan,
};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};

pub(super) fn source_id(
    plan: &LvmVolumePlan,
    fingerprint: &str,
    inputs: &[domain::structure::SourceRevisionRef],
) -> DataSourceId {
    DataSourceId(evidence_access::lvm::registered_source_id(
        &plan.vg_uuid,
        &plan.lv_uuid,
        fingerprint,
        inputs,
    ))
}

pub(super) fn register_lvm_volume(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &LvmVolumePlan,
    inputs: &[domain::structure::SourceRevisionRef],
) -> Result<DataSource, LvmPlanError> {
    super::inputs::validate_registered(conn, case_id, plan, inputs)?;
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let logical_size = open_lvm_volume_reader(plan, &cancelled)?.info().size;
    let fingerprint = lvm_volume_fingerprint(plan)?;
    let id = source_id(plan, &fingerprint, inputs);
    let repo = DataSourceRepo::new(conn);
    if let Some(existing) = repo
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == id)
    {
        if repo.case_id(&id)? != *case_id || existing.kind != DataSourceKind::LvmLogicalVolume {
            return Err(LvmPlanError::InvalidPublication);
        }
        let persisted = read_lvm_descriptor(&existing.source_path)?;
        if persisted != *plan {
            return Err(LvmPlanError::InvalidPublication);
        }
        persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(conn)
            .bind_inputs(&case_id.0, &existing.id.0, inputs)?;
        super::object_version::bind(conn, case_id, &existing, plan, &fingerprint, inputs)?;
        return Ok(existing);
    }
    let descriptor_path = write_lvm_descriptor(case_root, plan)?;
    let source = DataSource {
        id,
        name: format!("LVM {}/{}", plan.vg_name, plan.lv_name),
        kind: DataSourceKind::LvmLogicalVolume,
        source_path: descriptor_path.clone(),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance {
            source_hash_sha256: None,
            hash_status: DataSourceHashStatus::Unavailable,
            canonical_source_path: Some(std::fs::canonicalize(&descriptor_path)?),
            evidence_size: Some(logical_size),
            reader_kind: Some("lvm_logical_volume".to_string()),
            provenance_status: DataSourceProvenanceStatus::Recorded,
            warnings: Vec::new(),
        },
    };
    let transaction = conn.unchecked_transaction()?;
    DataSourceRepo::new(&transaction).insert_with_storage(
        case_id,
        &source,
        &DataSourceStorage::source_db(
            &source.id.0,
            Some("linux"),
            Some("lvm_logical_volume".to_string()),
        ),
    )?;
    persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(&transaction)
        .bind_inputs(&case_id.0, &source.id.0, inputs)?;
    super::object_version::bind(&transaction, case_id, &source, plan, &fingerprint, inputs)?;
    transaction.commit()?;
    Ok(source)
}
