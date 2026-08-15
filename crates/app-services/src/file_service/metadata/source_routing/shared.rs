use std::path::Path;

use domain::{CaseId, DataSourceId, FileEntryId};
use rusqlite::Connection;

use crate::{
    file_service::{FileServiceError, SourceReadContext},
    source_db::{GlobalFileId, SourceConnectionManager},
};

pub(super) fn source_manager(case_root: &Path) -> SourceConnectionManager {
    SourceConnectionManager::new(case_root.to_path_buf())
}

pub(super) fn open_source_for_data_source(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<Connection, FileServiceError> {
    Ok(source_manager(case_root).open_ready_read_only(case_conn, case_id, data_source_id)?)
}

pub(crate) fn open_source_for_file_id(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<(GlobalFileId, Connection), FileServiceError> {
    Ok(
        source_manager(case_root).open_ready_for_global_file_id_read_only(
            case_conn,
            case_id,
            &FileEntryId(file_id.to_string()),
        )?,
    )
}

pub(super) fn scoped_context<'a>(
    source_conn: &'a Connection,
    case_conn: &'a Connection,
    case_root: &'a Path,
    case_id: &'a CaseId,
    data_source_id: &'a DataSourceId,
) -> SourceReadContext<'a> {
    SourceReadContext::new(source_conn, case_conn, case_root, case_id, data_source_id)
}
