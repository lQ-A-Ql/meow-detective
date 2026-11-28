use domain::{
    structure::{
        NativeObjectId, NativeVersion, RevisionConsistency, SourceRevisionContent,
        SourceRevisionRef, StorageObjectVersion, StorageObjectVersionKind,
    },
    CaseId, DataSource, DataSourceId, DataSourceKind,
};
use evidence_access::btrfs_pool::{self, BtrfsPoolPlan, BtrfsSourceError};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_revision_repo::SourceRevisionRepo,
    storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn register(
    conn: &rusqlite::Connection,
    root: &Path,
    case_id: &CaseId,
    plan: &BtrfsPoolPlan,
    inputs: &[SourceRevisionRef],
) -> Result<DataSource, BtrfsSourceError> {
    btrfs_pool::validate_inputs(conn, &case_id.0, plan, inputs)?;
    drop(btrfs_pool::open_filesystem(
        plan,
        &std::sync::atomic::AtomicBool::new(false),
    )?);
    let fingerprint = plan.fingerprint()?;
    let mut parents = inputs.to_vec();
    parents.sort_by(|left, right| left.source_id.cmp(&right.source_id));
    let identity = hex::encode(Sha256::digest(serde_json::to_vec(&(
        &fingerprint,
        &parents,
    ))?));
    let id = DataSourceId(format!("btrfs-{identity}"));
    let existing = DataSourceRepo::new(conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == id);
    if let Some(existing) = existing {
        if existing.kind != DataSourceKind::BtrfsPool
            || btrfs_pool::read_descriptor(&existing.source_path)? != *plan
        {
            return Err(BtrfsSourceError::Identity);
        }
        return Ok(existing);
    }
    let descriptor = btrfs_pool::write_descriptor(root, plan)?;
    let source = DataSource {
        id,
        name: format!("Btrfs {}", plan.identity.devices[0].fsid),
        kind: DataSourceKind::BtrfsPool,
        source_path: descriptor,
        imported_at: chrono::Utc::now(),
        provenance: Default::default(),
    };
    let transaction = conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    DataSourceRepo::new(&transaction).insert_with_storage(
        case_id,
        &source,
        &DataSourceStorage::source_db(&source.id.0, Some("linux"), Some("btrfs_pool".into())),
    )?;
    SourceRevisionRepo::new(&transaction).bind_inputs(&case_id.0, &source.id.0, &parents)?;
    bind_version(&transaction, case_id, &source, plan, &fingerprint, &parents)?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(source)
}

fn bind_version(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    source: &DataSource,
    plan: &BtrfsPoolPlan,
    fingerprint: &str,
    inputs: &[SourceRevisionRef],
) -> Result<(), BtrfsSourceError> {
    let native = &plan.identity.devices[0];
    let scope = format!(
        "{}:{}",
        case_id.0,
        inputs
            .iter()
            .map(|input| input.source_id.as_str())
            .collect::<Vec<_>>()
            .join("\u{1f}")
    );
    let object = NativeObjectId::new("fs-btrfs", "btrfs.fsid", native.fsid.clone(), scope.clone())
        .map_err(|_| BtrfsSourceError::Identity)?;
    let acquisitions = inputs
        .iter()
        .map(|input| {
            SourceRevisionRepo::new(conn)
                .current(&case_id.0, &input.source_id)
                .map(|(_, content)| content.acquisition_id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let version = StorageObjectVersion::new(
        object.clone(),
        acquisitions,
        source.id.0.clone(),
        source.id.0.clone(),
        btrfs_pool::ADAPTER_VERSION.into(),
        btrfs_pool::POLICY_VERSION.into(),
        fingerprint.into(),
        inputs.to_vec(),
        vec![NativeVersion::Btrfs {
            fsid: native.fsid.clone(),
            generation: native.generation,
            device_count: native.device_count,
            mapping_sha256: plan.identity.mapping_sha256.clone(),
        }],
        RevisionConsistency::Coherent,
        StorageObjectVersionKind::DerivedOutput,
    )
    .map_err(|_| BtrfsSourceError::Identity)?;
    let objects = StorageObjectRepo::new(conn);
    objects.insert_if_absent(&StorageObjectRecord {
        id: object.storage_object_id(),
        case_id: case_id.0.clone(),
        object_kind: "file_system".into(),
        name: source.name.clone(),
        identity_state: "proven".into(),
        status: "discovered".into(),
        provenance_json: serde_json::to_string(&plan.identity)?,
    })?;
    objects.bind_version(&case_id.0, &version)?;
    SourceRevisionRepo::new(conn).bind(
        &case_id.0,
        &source.id.0,
        &SourceRevisionContent::derived(
            domain::structure::AcquisitionId(scope),
            plan.identity.total_bytes,
            version.id.0,
        ),
        true,
    )?;
    Ok(())
}
