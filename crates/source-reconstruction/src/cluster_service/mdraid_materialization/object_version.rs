use domain::{
    structure::{
        AcquisitionId, NativeObjectId, RevisionConsistency, SourceRevisionContent,
        StorageObjectVersion, StorageObjectVersionKind,
    },
    CaseId, DataSource, StorageObjectKind,
};
use evidence_access::mdraid::{MdRaidArrayPlan, MdRaidPlanError};
use persistence_sqlite::repositories::{
    source_revision_repo::SourceRevisionRepo,
    storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
};

const ADAPTER_VERSION: &str = "fs-lvm/mdraid-v3";
const POLICY_VERSION: &str = "mdraid-policy-v1";

pub(super) fn bind(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    source: &DataSource,
    plan: &MdRaidArrayPlan,
    fingerprint: &str,
    logical_size: u64,
    inputs: &[domain::structure::SourceRevisionRef],
) -> Result<(), MdRaidPlanError> {
    let native_object_id = native_object_id(case_id, plan, inputs)?;
    let acquisition_id = AcquisitionId(native_object_id.scope.clone());
    let object_id = native_object_id.storage_object_id();
    let repo = StorageObjectRepo::new(conn);
    repo.insert_if_absent(&StorageObjectRecord {
        id: object_id.clone(),
        case_id: case_id.0.clone(),
        object_kind: StorageObjectKind::MdRaid.as_str().into(),
        name: source.name.clone(),
        identity_state: "proven".into(),
        status: "discovered".into(),
        provenance_json: serde_json::json!({
            "arrayUuid": hex::encode(plan.array_uuid),
            "acquisitionScope": native_object_id.scope,
            "versionFingerprint": fingerprint,
        })
        .to_string(),
    })?;
    let mut native_versions = Vec::new();
    for member in &plan.members {
        let version = evidence_access::mdraid::mdraid_native_version(&member.superblock);
        if !native_versions.contains(&version) {
            native_versions.push(version);
        }
    }
    let version = StorageObjectVersion::new(
        native_object_id,
        SourceRevisionRepo::new(conn).acquisition_ids(inputs)?,
        source.id.0.clone(),
        source.id.0.clone(),
        ADAPTER_VERSION.into(),
        POLICY_VERSION.into(),
        fingerprint.into(),
        inputs.to_vec(),
        native_versions,
        RevisionConsistency::Coherent,
        StorageObjectVersionKind::DerivedOutput,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    repo.bind_version(&case_id.0, &version)?;
    bind_derived_revision(
        conn,
        case_id,
        source,
        &version.id.0,
        logical_size,
        acquisition_id,
    )?;
    Ok(())
}

pub(super) fn bind_derived_revision(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    source: &DataSource,
    version_id: &str,
    logical_size: u64,
    acquisition_id: AcquisitionId,
) -> Result<(), MdRaidPlanError> {
    SourceRevisionRepo::new(conn).bind(
        &case_id.0,
        &source.id.0,
        &SourceRevisionContent::derived(acquisition_id, logical_size, version_id.to_owned()),
        true,
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
    if version.mapping_sha256 != fingerprint
        || version.adapter_version != ADAPTER_VERSION
        || version.policy_version != POLICY_VERSION
        || version.version_kind != StorageObjectVersionKind::DerivedOutput
    {
        return Err(MdRaidPlanError::InvalidPublication);
    }
    SourceRevisionRepo::new(conn).require_current_inputs(case_id, source_id)?;
    Ok(())
}

fn native_object_id(
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
    inputs: &[domain::structure::SourceRevisionRef],
) -> Result<NativeObjectId, persistence_sqlite::DbError> {
    let mut source_ids = inputs
        .iter()
        .map(|input| input.source_id.as_str())
        .collect::<Vec<_>>();
    source_ids.sort_unstable();
    source_ids.dedup();
    let scope = format!("{}:{}", case_id.0, source_ids.join("\u{1f}"));
    NativeObjectId::new(
        "fs-lvm",
        "mdraid.array_uuid",
        hex::encode(plan.array_uuid),
        scope,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
}
