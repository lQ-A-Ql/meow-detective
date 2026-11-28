use std::path::Path;

use evidence_core::FileSystemReader;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;

use super::{
    viewer::{partition::is_previewable_partition_status, preview_partition_candidate_from_record},
    FileServiceError,
};

pub(crate) fn open_registered_partition(
    source_id: &str,
    descriptor: &Path,
    partition_index: Option<usize>,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    let descriptor = descriptor.canonicalize()?;
    let root = descriptor
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| FileServiceError::security("derived descriptor has no case workspace"))?;
    let conn = persistence_sqlite::open_existing_case_read_only(&root.join("app.db"))?;
    let id = domain::DataSourceId(source_id.into());
    let sources = DataSourceRepo::new(&conn);
    let case_id = sources.case_id(&id)?;
    if Path::new(&sources.source_path(&id)?).canonicalize()? != descriptor {
        return Err(FileServiceError::security(
            "derived descriptor differs from registered source",
        ));
    }
    let reference =
        persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(&conn)
            .current(&case_id.0, source_id)?
            .0;
    let scope = crate::structure_access::acquire_registered_scope(&conn, root, &reference)
        .map_err(|error| FileServiceError::Io(std::io::Error::other(error)))?;
    let partitions = crate::partition_query_service::PartitionQueryRepo::new(&conn)
        .find_by_data_source(source_id)?;
    let candidates = partitions
        .iter()
        .filter(|partition| {
            partition.filesystem.is_some()
                && is_previewable_partition_status(&partition.status)
                && partition_index.is_none_or(|index| partition.partition_index as usize == index)
        })
        .collect::<Vec<_>>();
    let [partition] = candidates.as_slice() else {
        return Err(FileServiceError::other(
            "derived source requires one published filesystem identity",
        ));
    };
    let reader = crate::source_runtime::open_factory_for_case(
        &descriptor,
        &sources.source_kind(&id)?,
        &std::sync::atomic::AtomicBool::new(false),
        &case_id.0,
    )?
    .open_reader()?;
    let preview = preview_partition_candidate_from_record(partition);
    super::native_filesystem::open_preview_filesystem(scope.wrap(reader)?, &preview)
}
