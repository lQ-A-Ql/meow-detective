use domain::{
    structure::{
        AcquisitionId, NativeObjectId, RevisionConsistency, SourceRevisionContent,
        StorageObjectVersion, StorageObjectVersionKind,
    },
    CaseId, DataSource, StorageObjectKind,
};
use evidence_access::lvm::{
    lvm_native_version, LvmPlanError, LvmVolumePlan, LVM_ADAPTER_VERSION, LVM_POLICY_VERSION,
};
use persistence_sqlite::repositories::{
    source_revision_repo::SourceRevisionRepo,
    storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
};

pub(super) fn bind(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    source: &DataSource,
    plan: &LvmVolumePlan,
    fingerprint: &str,
    inputs: &[domain::structure::SourceRevisionRef],
) -> Result<(), LvmPlanError> {
    let native_object_id = native_object_id(case_id, plan, inputs)?;
    let acquisition_id = AcquisitionId(native_object_id.scope.clone());
    let object_id = native_object_id.storage_object_id();
    let repo = StorageObjectRepo::new(conn);
    repo.insert_if_absent(&StorageObjectRecord {
        id: object_id.clone(),
        case_id: case_id.0.clone(),
        object_kind: StorageObjectKind::LogicalVolume.as_str().into(),
        name: source.name.clone(),
        identity_state: "proven".into(),
        status: "discovered".into(),
        provenance_json: serde_json::json!({
            "vgUuid": plan.vg_uuid,
            "lvUuid": plan.lv_uuid,
            "acquisitionScope": native_object_id.scope,
            "versionFingerprint": fingerprint,
        })
        .to_string(),
    })?;
    let version = StorageObjectVersion::new(
        native_object_id,
        acquisition_ids(inputs),
        source.id.0.clone(),
        source.id.0.clone(),
        LVM_ADAPTER_VERSION.into(),
        LVM_POLICY_VERSION.into(),
        fingerprint.into(),
        inputs.to_vec(),
        vec![lvm_native_version(plan)],
        RevisionConsistency::Coherent,
        StorageObjectVersionKind::DerivedOutput,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    repo.bind_version(&case_id.0, &version)?;
    SourceRevisionRepo::new(conn).bind(
        &case_id.0,
        &source.id.0,
        &SourceRevisionContent::derived(acquisition_id, plan.logical_size, version.id.0.clone()),
        true,
    )?;
    Ok(())
}

pub(super) fn validate(
    conn: &rusqlite::Connection,
    case_id: &str,
    source_id: &str,
    fingerprint: &str,
) -> Result<(), LvmPlanError> {
    let version = StorageObjectRepo::new(conn)
        .version_for_source(case_id, source_id)?
        .ok_or(LvmPlanError::InvalidPublication)?;
    if version.mapping_sha256 != fingerprint
        || version.adapter_version != LVM_ADAPTER_VERSION
        || version.policy_version != LVM_POLICY_VERSION
        || version.version_kind != StorageObjectVersionKind::DerivedOutput
    {
        return Err(LvmPlanError::InvalidPublication);
    }
    SourceRevisionRepo::new(conn).require_current_inputs(case_id, source_id)?;
    Ok(())
}

fn native_object_id(
    case_id: &CaseId,
    plan: &LvmVolumePlan,
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
        "lvm.lv_uuid",
        evidence_access::datasource_service::normalize_lvm_uuid_for_match(&plan.lv_uuid),
        scope,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
}

fn acquisition_ids(inputs: &[domain::structure::SourceRevisionRef]) -> Vec<AcquisitionId> {
    let mut source_ids = inputs
        .iter()
        .map(|input| input.source_id.clone())
        .collect::<Vec<_>>();
    source_ids.sort();
    source_ids.dedup();
    source_ids.into_iter().map(AcquisitionId).collect()
}
