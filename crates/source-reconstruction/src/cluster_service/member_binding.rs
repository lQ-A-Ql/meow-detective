use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRepo;
use rusqlite::Connection;

use super::{ClusterServiceError, Result};

pub fn get_linux_import_member_source_id(
    connection: &Connection,
    case_id: &CaseId,
    import_set_id: &str,
    member_index: usize,
) -> Result<Option<DataSourceId>> {
    let member_index =
        u32::try_from(member_index).map_err(|_| ClusterServiceError::InvalidMemberIndex)?;
    Ok(LinuxImportSetRepo::new(connection)
        .find_member_source_id(&case_id.0, import_set_id, member_index)?
        .map(DataSourceId))
}
