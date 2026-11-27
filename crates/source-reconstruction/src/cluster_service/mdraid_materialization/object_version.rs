use domain::{
    structure::{NativeVersion, StorageObjectVersion},
    CaseId, DataSource, StorageObjectKind,
};
use evidence_access::mdraid::{MdRaidArrayPlan, MdRaidPlanError};
use persistence_sqlite::repositories::{
    source_revision_repo::SourceRevisionRepo,
    storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
};

pub(super) fn bind(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    source: &DataSource,
    plan: &MdRaidArrayPlan,
    fingerprint: &str,
) -> Result<(), MdRaidPlanError> {
    // The derived source identity already includes the acquisition-bound inputs.
    let object_id = format!("storage:{}", source.id.0);
    let repo = StorageObjectRepo::new(conn);
    repo.insert_if_absent(&StorageObjectRecord {
        id: object_id.clone(),
        case_id: case_id.0.clone(),
        object_kind: StorageObjectKind::MdRaid.as_str().into(),
        name: source.name.clone(),
        identity_state: "proven".into(),
        status: "discovered".into(),
        provenance_json:
            serde_json::json!({"arrayUuid": hex::encode(plan.array_uuid), "version": fingerprint})
                .to_string(),
    })?;
    let mut native_versions = Vec::new();
    for member in &plan.members {
        let version = NativeVersion::Md {
            metadata_version: format!("{:?}", member.superblock.metadata_version),
            events: member.superblock.events,
        };
        if !native_versions.contains(&version) {
            native_versions.push(version);
        }
    }
    repo.bind_version(
        &case_id.0,
        &StorageObjectVersion {
            id: format!("version:{}", source.id.0),
            storage_object_id: object_id,
            derived_source_id: source.id.0.clone(),
            adapter_version: "fs-lvm/mdraid-v2".into(),
            mapping_sha256: fingerprint.into(),
            native_versions,
        },
    )?;
    Ok(())
}

pub(super) fn validate(
    conn: &rusqlite::Connection,
    case_id: &str,
    source_id: &str,
    fingerprint: &str,
) -> Result<(), MdRaidPlanError> {
    let version = StorageObjectRepo::new(conn)
        .version_for_source(case_id, source_id)?
        .ok_or(MdRaidPlanError::InvalidPublication)?;
    if version.mapping_sha256 != fingerprint || version.adapter_version != "fs-lvm/mdraid-v2" {
        return Err(MdRaidPlanError::InvalidPublication);
    }
    SourceRevisionRepo::new(conn).require_current_inputs(case_id, source_id)?;
    Ok(())
}
