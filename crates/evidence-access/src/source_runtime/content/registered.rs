use std::path::Path;

use domain::structure::{PhysicalContentBinding, SourceRevisionRef};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};
use sha2::{Digest, Sha256};

use super::SourceContent;

pub fn persist_registered_content(
    conn: &rusqlite::Connection,
    source: &SourceRevisionRef,
    content: &SourceContent,
) -> persistence_sqlite::DbResult<()> {
    content.validate().map_err(error)?;
    let sources = [crate::datasource_service::LvmDiscoverySource::new(
        &content.source_path,
        content.source_kind.clone(),
    )];
    let snapshot =
        crate::source_runtime::snapshot::SourceSnapshot::capture_images(&sources).map_err(error)?;
    let payload = serde_json::to_vec(content).map_err(error)?;
    persistence_source::detail::write_physical_content(
        conn,
        &PhysicalContentBinding {
            source: source.clone(),
            logical_size: content.logical_size,
            logical_sha256: content.logical_sha256.clone(),
            manifest_sha256: hex::encode(Sha256::digest(&payload)),
            backing_paths: snapshot.paths().map(Path::to_path_buf).collect(),
        },
        &payload,
    )
}

pub(crate) fn load_registered_content(
    conn: &rusqlite::Connection,
    root: &Path,
    source: &SourceRevisionRef,
) -> persistence_sqlite::DbResult<(
    SourceContent,
    source_kernel::resource_budget::ResourcePermit,
)> {
    SourceRevisionRepo::new(conn).require_current(source)?;
    let (_, revision) =
        SourceRevisionRepo::new(conn).current(&source.case_id, &source.source_id)?;
    let bytes = SourceContent::estimated_bytes(revision.logical_size).map_err(error)?;
    let permit = source_kernel::resource_budget::global_pool()
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes,
                metadata: bytes,
                ..Default::default()
            },
            None,
        )
        .map_err(error)?;
    let control = source_kernel::source_db::open_registered_source_db_read_only(
        conn,
        root,
        &domain::DataSourceId(source.source_id.clone()),
    )?;
    let (binding, payload) = persistence_source::detail::read_physical_content(&control, source)?;
    let content: SourceContent = serde_json::from_slice(&payload).map_err(error)?;
    content.validate().map_err(error)?;
    let sources = DataSourceRepo::new(conn);
    let id = domain::DataSourceId(source.source_id.clone());
    if content.logical_size != revision.logical_size
        || revision.logical_sha256.as_deref() != Some(content.logical_sha256.as_str())
        || binding.logical_size != content.logical_size
        || binding.logical_sha256 != content.logical_sha256
        || sources.source_kind(&id)? != content.source_kind
        || Path::new(&sources.source_path(&id)?)
            .canonicalize()
            .map_err(error)?
            != Path::new(&content.source_path)
                .canonicalize()
                .map_err(error)?
    {
        return Err(error(
            "registered physical content differs from source revision",
        ));
    }
    Ok((content, permit))
}

pub(crate) fn physical_backing_paths(
    conn: &rusqlite::Connection,
    root: &Path,
    source: &SourceRevisionRef,
) -> persistence_sqlite::DbResult<Vec<std::path::PathBuf>> {
    let mut paths = std::collections::BTreeSet::new();
    for dependency in SourceRevisionRepo::new(conn).dependency_revisions(source)? {
        let id = domain::DataSourceId(dependency.source_id.clone());
        if !matches!(
            DataSourceRepo::new(conn).source_kind(&id)?,
            domain::DataSourceKind::Raw | domain::DataSourceKind::E01
        ) {
            continue;
        }
        let control =
            source_kernel::source_db::open_registered_source_db_read_only(conn, root, &id)?;
        let binding =
            persistence_source::detail::read_physical_content_binding(&control, &dependency)?;
        let (_, revision) =
            SourceRevisionRepo::new(conn).current(&dependency.case_id, &dependency.source_id)?;
        if binding.logical_size != revision.logical_size
            || revision.logical_sha256.as_deref() != Some(binding.logical_sha256.as_str())
        {
            return Err(error(
                "physical backing identity differs from source revision",
            ));
        }
        paths.extend(binding.backing_paths);
    }
    Ok(paths.into_iter().collect())
}

fn error(error: impl std::fmt::Display) -> persistence_sqlite::DbError {
    persistence_sqlite::DbError::System(error.to_string())
}
