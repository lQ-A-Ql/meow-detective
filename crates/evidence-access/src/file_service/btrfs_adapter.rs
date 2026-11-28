use super::{FileServiceError, PreviewDescriptor, RangeContentReader};
use domain::{structure::SourceRevisionRef, CaseId, DataSourceId, FileEntryId};
use std::path::Path;

pub(crate) fn open_evidence_file(
    conn: &rusqlite::Connection,
    root: &Path,
    source: &SourceRevisionRef,
    file_id: &FileEntryId,
) -> Result<Box<dyn evidence_core::EvidenceReader>, FileServiceError> {
    let source_id = DataSourceId(source.source_id.clone());
    let case_id = CaseId(source.case_id.clone());
    let ready = source_kernel::source_db::open_ready_source_read_only_by_id(
        conn, root, &case_id, &source_id,
    )
    .map_err(|error| error.into_db_error())?;
    let descriptor =
        super::viewer::preview_descriptor_for_case(&ready.connection, &case_id.0, file_id)?;
    if descriptor.data_source_id != source.source_id || descriptor.source_kind != "btrfs_pool" {
        return Err(FileServiceError::security(
            "Btrfs file target belongs to another source",
        ));
    }
    let path = descriptor
        .path
        .strip_prefix("[P0]/")
        .unwrap_or(&descriptor.path);
    crate::btrfs_pool::open_file(conn, root, source, path, descriptor.size)
        .map_err(|error| FileServiceError::Io(std::io::Error::other(error)))
}

pub(super) fn open_descriptor(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    let path = Path::new(&descriptor.source_path);
    let root = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| FileServiceError::security("Btrfs descriptor has no case workspace"))?;
    let case = persistence_sqlite::open_existing_case_read_only(&root.join("app.db"))?;
    let repo = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&case);
    let source_id = DataSourceId(descriptor.data_source_id.clone());
    if Path::new(&repo.source_path(&source_id)?) != path {
        return Err(FileServiceError::security(
            "Btrfs descriptor differs from its registered source",
        ));
    }
    let (reference, _) =
        persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(&case)
            .current(&descriptor.case_id, &descriptor.data_source_id)?;
    Ok(RangeContentReader::Seekable(Box::new(open_evidence_file(
        &case,
        root,
        &reference,
        &FileEntryId(descriptor.file_id.clone()),
    )?)))
}
