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

pub(super) fn ready_import_members(
    connection: &Connection,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<Vec<(u32, String)>> {
    let repo = LinuxImportSetRepo::new(connection);
    let members = repo.find_members(import_set_id)?;
    if members.is_empty() {
        return Err(ClusterServiceError::InvalidClusterId);
    }
    members
        .into_iter()
        .map(|member| {
            if !matches!(member.import_state.as_str(), "ready" | "ready_metadata") {
                return Err(ClusterServiceError::IncompleteImportSet);
            }
            let source = repo
                .find_member_source_id(&case_id.0, import_set_id, member.member_index)?
                .ok_or(ClusterServiceError::IncompleteImportSet)?;
            let storage =
                persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(connection)
                    .find_storage(&DataSourceId(source.clone()))?
                    .ok_or(ClusterServiceError::IncompleteImportSet)?;
            if !matches!(storage.import_state.as_str(), "ready" | "ready_metadata") {
                return Err(ClusterServiceError::IncompleteImportSet);
            }
            Ok((member.member_index, source))
        })
        .collect()
}
