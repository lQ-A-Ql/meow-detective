use chrono::Utc;
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use evidence_access::mdraid::{
    mdraid_array_fingerprint, open_mdraid_reader, write_mdraid_descriptor, MdRaidArrayPlan,
    MdRaidPlanError,
};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};

pub(super) fn source_id(plan: &MdRaidArrayPlan, fingerprint: &str) -> DataSourceId {
    DataSourceId(format!(
        "mdraid-{}-{fingerprint}",
        hex::encode(plan.array_uuid)
    ))
}

pub(super) fn register_mdraid_array(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
) -> Result<DataSource, MdRaidPlanError> {
    let logical_size = open_mdraid_reader(plan)?.logical_size();
    let fingerprint = mdraid_array_fingerprint(plan)?;
    let id = source_id(plan, &fingerprint);
    let repo = DataSourceRepo::new(conn);
    if let Some(existing) = repo
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == id)
    {
        if repo.case_id(&id)? != *case_id || existing.kind != DataSourceKind::MdRaid {
            return Err(MdRaidPlanError::InvalidPublication);
        }
        let persisted = evidence_access::mdraid::read_mdraid_descriptor(&existing.source_path)?;
        if persisted != *plan {
            return Err(MdRaidPlanError::InvalidPublication);
        }
        return Ok(existing);
    }
    let descriptor_path = write_mdraid_descriptor(case_root, plan)?;
    let level = if plan.level == -1 {
        "linear".to_string()
    } else {
        plan.level.to_string()
    };
    let source = DataSource {
        id,
        name: format!("MD RAID{level} {}", hex::encode(plan.array_uuid)),
        kind: DataSourceKind::MdRaid,
        source_path: descriptor_path.clone(),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance {
            source_hash_sha256: None,
            hash_status: DataSourceHashStatus::Unavailable,
            canonical_source_path: Some(std::fs::canonicalize(&descriptor_path)?),
            evidence_size: Some(logical_size),
            reader_kind: Some("md_raid".to_string()),
            provenance_status: DataSourceProvenanceStatus::Recorded,
            warnings: Vec::new(),
        },
    };
    repo.insert_with_storage(
        case_id,
        &source,
        &DataSourceStorage::source_db(&source.id.0, Some("linux"), Some("mdraid".to_string())),
    )?;
    Ok(source)
}
