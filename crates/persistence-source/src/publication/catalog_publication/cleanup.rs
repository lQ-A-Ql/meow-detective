use std::path::Path;

use domain::DataSourceId;
use persistence_sqlite::repositories::catalog_publication_repo::CatalogPublicationRepo;

use super::StructurePublicationError;

pub fn discard_unpublished_catalog_build(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source_id: &DataSourceId,
    attempt_id: &str,
) -> Result<(), StructurePublicationError> {
    if CatalogPublicationRepo::new(conn)
        .find(source_id)?
        .is_some_and(|publication| publication.attempt_id == attempt_id)
    {
        return Ok(());
    }
    crate::publication::build::discard_source_build_db(case_root, source_id, attempt_id)?;
    let layout =
        persistence_fjall::SourceBuildLayout::for_case_source(case_root, &source_id.0, attempt_id)?;
    let path = layout.build_path();
    if path.exists() {
        let parent = crate::control::paths::source_dir(case_root, source_id).canonicalize()?;
        let resolved = path.canonicalize()?;
        if resolved.parent() != Some(parent.as_path()) {
            return Err(StructurePublicationError::InvalidPublication);
        }
        let _guard = crate::runtime::close_source_store_at_path(&path)?
            .ok_or(StructurePublicationError::InvalidPublication)?;
        std::fs::remove_dir_all(resolved)?;
    }
    Ok(())
}
