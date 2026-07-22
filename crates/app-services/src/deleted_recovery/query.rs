use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::deleted_recovery_repo::DeletedRecoveryRepo;
use rusqlite::Connection;
use transport::dto::DeletedRecoveryPageDto;

use super::{mapping::page_to_dto, DeletedRecoveryError};

pub fn list_deleted_recoveries(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    offset: u64,
    limit: u32,
) -> Result<DeletedRecoveryPageDto, DeletedRecoveryError> {
    let source = crate::source_db::open_ready_source_read_only_by_id(
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )?;
    let page = DeletedRecoveryRepo::new(&source.connection)
        .list_page(&data_source_id.0, partition_index, offset, limit)?
        .ok_or_else(|| DeletedRecoveryError::NotFound {
            data_source_id: data_source_id.0.clone(),
            partition_index,
        })?;
    page_to_dto(page)
}
