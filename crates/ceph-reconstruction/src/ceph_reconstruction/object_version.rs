use domain::structure::{
    NativeObjectId, NativeVersion, StorageObjectVersion, StorageObjectVersionKind,
};
use persistence_sqlite::{
    repositories::{
        source_revision_repo::SourceRevisionRepo,
        storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
    },
    DbError, DbResult,
};

fn ceph_native_object_id(adapter: &str, scope: &str, native_id: &str) -> NativeObjectId {
    NativeObjectId {
        adapter: adapter.into(),
        identity_space: format!("{adapter}.native_id"),
        native_id: native_id.into(),
        scope: scope.into(),
    }
}

pub fn cephfs_native_object_id(scope: &str, cluster: &str, filesystem_id: i64) -> NativeObjectId {
    ceph_native_object_id(
        "ceph-fs",
        scope,
        &serde_json::json!([cluster, filesystem_id]).to_string(),
    )
}

pub fn rbd_native_object_id(scope: &str, descriptor: &super::RbdImageDescriptor) -> NativeObjectId {
    ceph_native_object_id(
        "ceph-rbd",
        scope,
        &serde_json::json!([
            descriptor.scope_identity,
            descriptor.metadata.data_pool_id,
            descriptor.metadata.id,
        ])
        .to_string(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn bind_ceph_object_version(
    conn: &rusqlite::Connection,
    case_id: &str,
    source_id: &str,
    native_id: NativeObjectId,
    name: &str,
    mapping_sha256: &str,
    native_version: NativeVersion,
) -> DbResult<()> {
    let transaction = conn
        .is_autocommit()
        .then(|| conn.unchecked_transaction())
        .transpose()?;
    let revisions = SourceRevisionRepo::new(conn);
    let (_, output) = revisions.current(case_id, source_id)?;
    let parents = revisions.inputs(case_id, source_id)?;
    let acquisitions = revisions.acquisition_ids(&parents)?;
    let version = StorageObjectVersion::new(
        native_id,
        acquisitions,
        source_id.into(),
        source_id.into(),
        "ceph-reconstruction/1".into(),
        "ceph-lineage/1".into(),
        mapping_sha256.into(),
        parents,
        vec![native_version],
        output.consistency,
        StorageObjectVersionKind::DerivedOutput,
    )
    .map_err(|error| DbError::System(error.to_string()))?;
    let repo = StorageObjectRepo::new(conn);
    repo.insert_if_absent(&StorageObjectRecord {
        id: version.storage_object_id.clone(),
        case_id: case_id.into(),
        object_kind: version.native_object_id.adapter.replace('-', "_"),
        name: name.into(),
        identity_state: "candidate".into(),
        status: "discovered".into(),
        provenance_json: serde_json::json!({
            "sourceId": source_id,
            "parentRevisions": version.parent_source_revisions,
        })
        .to_string(),
    })?;
    repo.bind_version(case_id, &version)?;
    if let Some(transaction) = transaction {
        transaction.commit()?;
    }
    Ok(())
}
