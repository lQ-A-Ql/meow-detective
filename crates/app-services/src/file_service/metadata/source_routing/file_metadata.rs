use std::path::Path;

use domain::{CaseId, DataSourceId, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::dto::FileEntryRowDto;

use super::shared::open_source_for_file_id;
use crate::{file_service::FileServiceError, source_db::GlobalFileId};

pub fn get_file_metadata_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<FileEntryRowDto, FileServiceError> {
    let (global, conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let entry = FileRepo::new(&conn)
        .find_by_id(&global.local_id)?
        .ok_or_else(|| FileServiceError::not_found("File entry does not exist"))?;
    scoped_row(&entry, &global.data_source_id)
}

pub(super) fn scoped_row(
    entry: &FileEntry,
    source_id: &DataSourceId,
) -> Result<FileEntryRowDto, FileServiceError> {
    if entry.data_source_id != *source_id {
        return Err(FileServiceError::security(
            "File entry source does not match",
        ));
    }
    let mut row = super::super::lookup::file_entry_to_dto(entry);
    row.id = GlobalFileId::new(source_id.clone(), entry.id.clone())
        .encode()
        .0;
    row.parent_id = row.parent_id.map(|id| {
        GlobalFileId::new(source_id.clone(), FileEntryId(id))
            .encode()
            .0
    });
    Ok(row)
}
