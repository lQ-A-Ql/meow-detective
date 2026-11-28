use std::io;

use domain::{structure::SourceRevisionRef, DataSourceId};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo, datasource_repo::DataSourceRepo,
    source_revision_repo::SourceRevisionRepo, storage_object_repo::StorageObjectRepo,
    structure_view_attempt_repo::StructureViewAttemptRepo,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(PartialEq, Eq)]
pub(super) struct ScopeSnapshot {
    dependencies: Vec<(SourceRevisionRef, String, String, String)>,
    versions: String,
    publication: String,
}

pub(super) fn capture(
    conn: &rusqlite::Connection,
    source: &SourceRevisionRef,
    dependencies: &[SourceRevisionRef],
) -> io::Result<ScopeSnapshot> {
    let revisions = SourceRevisionRepo::new(conn);
    if revisions
        .dependency_revisions(source)
        .map_err(io::Error::other)?
        != dependencies
    {
        return Err(io::Error::other("source dependency revisions changed"));
    }
    let sources = DataSourceRepo::new(conn);
    let source_id = DataSourceId(source.source_id.clone());
    let bindings = dependencies
        .iter()
        .map(|reference| {
            let id = DataSourceId(reference.source_id.clone());
            let storage = sources.find_storage(&id).map_err(io::Error::other)?
                .ok_or_else(|| io::Error::other("source dependency registration is missing"))?;
            let (_, revision) = revisions.current(&reference.case_id, &reference.source_id).map_err(io::Error::other)?;
            let physical = matches!(sources.source_kind(&id).map_err(io::Error::other)?,
                domain::DataSourceKind::Raw | domain::DataSourceKind::E01)
                && revision.logical_sha256_status == domain::structure::DigestVerification::Verified;
            if !physical && !matches!(storage.import_state.as_str(), "ready" | "ready_metadata") {
                return Err(io::Error::other("source dependency has neither published data nor verified physical content"));
            }
            Ok((
                reference.clone(),
                sources.source_path(&id).map_err(io::Error::other)?,
                sources
                    .source_kind(&id)
                    .map_err(io::Error::other)?
                    .to_string(),
                serde_json::json!({
                    "sourceDb": storage.source_db_rel_path,
                    "model": storage.storage_model,
                    "schema": storage.schema_version,
                    "state": if physical && reference != source { "verified_physical_content" } else { storage.import_state.as_str() },
                    "platform": storage.platform,
                })
                .to_string(),
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let versions = StorageObjectRepo::new(conn)
        .versions_for_source(&source.case_id, &source.source_id)
        .map_err(io::Error::other)?;
    if matches!(
        sources.source_kind(&source_id).map_err(io::Error::other)?,
        domain::DataSourceKind::MdRaid
            | domain::DataSourceKind::LvmLogicalVolume
            | domain::DataSourceKind::BtrfsPool
            | domain::DataSourceKind::CephRbd
            | domain::DataSourceKind::CephFs
    ) && versions.is_empty()
    {
        return Err(io::Error::other(
            "derived source has no public object version",
        ));
    }
    let mut version_states = Vec::with_capacity(versions.len());
    for version in versions {
        let object = StorageObjectRepo::new(conn)
            .find(&version.storage_object_id)
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("source object is missing"))?;
        if version.consistency == domain::structure::RevisionConsistency::Conflicted {
            return Err(io::Error::other("source object version is conflicted"));
        }
        version_states
            .push(serde_json::json!({"version": version, "identityState": object.identity_state}));
    }
    let publication = CatalogPublicationRepo::new(conn)
        .find(&source_id)
        .map_err(io::Error::other)?;
    let attempt = StructureViewAttemptRepo::new(conn)
        .find(&source.case_id, &source.source_id)
        .map_err(io::Error::other)?;
    let publication = serde_json::json!({
        "attempt": attempt,
        "publication": publication.map(|value| serde_json::json!({
            "attempt": value.attempt_id, "fingerprint": value.input_fingerprint,
            "receipt": value.catalog_digest, "seal": value.seal, "state": value.state,
            "path": value.source_db_rel_path,
        })),
    })
    .to_string();
    Ok(ScopeSnapshot {
        dependencies: bindings,
        versions: serde_json::to_string(&version_states).map_err(io::Error::other)?,
        publication,
    })
}

pub(super) fn data_version(conn: &rusqlite::Connection) -> io::Result<u64> {
    conn.pragma_query_value(None, "data_version", |row| row.get(0))
        .map_err(io::Error::other)
}

pub(super) fn fingerprint(snapshot: &ScopeSnapshot) -> io::Result<String> {
    #[derive(Serialize)]
    struct Identity<'identity> {
        dependencies: &'identity [(SourceRevisionRef, String, String, String)],
        versions: &'identity str,
        publication: &'identity str,
    }
    let bytes = serde_json::to_vec(&Identity {
        dependencies: &snapshot.dependencies,
        versions: &snapshot.versions,
        publication: &snapshot.publication,
    })
    .map_err(io::Error::other)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(super) fn manifest_identity(
    native: &persistence_source::runtime::ReadOnlySourceStorageHandle,
    source: &str,
) -> io::Result<String> {
    let snapshot = native.snapshot();
    native
        .require_projection_complete(&snapshot, source)
        .map_err(io::Error::other)?;
    let manifest = native
        .generation_manifest(source)
        .map_err(io::Error::other)?;
    Ok(serde_json::json!({
        "attempt": manifest.attempt_id,
        "immutable": manifest.immutable_data_digest,
        "projection": manifest.projection_batch_digest,
    })
    .to_string())
}
