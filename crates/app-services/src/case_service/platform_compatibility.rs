use super::{CaseServiceError, Result};
use crate::active_case::ActiveCase;
use domain::{CaseId, DataSourcePlatform, DataSourcePlatformParseError};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;

pub fn ensure_supported_data_source_platforms(active: &ActiveCase) -> Result<()> {
    let unsupported = active.with_conn(|conn| unsupported_platform(conn, &active.meta.id))?;

    reject_unsupported_platform(unsupported)
}

pub(super) fn ensure_supported_data_source_platforms_for_case(
    conn: &Connection,
    case_id: &CaseId,
) -> Result<()> {
    reject_unsupported_platform(unsupported_platform(conn, case_id)?)
}

fn unsupported_platform(
    conn: &Connection,
    case_id: &CaseId,
) -> persistence_sqlite::DbResult<Option<String>> {
    let repo = DataSourceRepo::new(conn);
    for source in repo.find_by_case(case_id)? {
        let Some(storage) = repo.find_storage(&source.id)? else {
            return Ok(Some("missing storage metadata".to_string()));
        };
        match DataSourcePlatform::from_storage_str(Some(&storage.platform)) {
            Ok(DataSourcePlatform::Windows | DataSourcePlatform::Linux) => {}
            Ok(DataSourcePlatform::Unknown) => {
                return Ok(Some(platform_label(&storage.platform)));
            }
            Err(DataSourcePlatformParseError::UnsupportedValue { value }) => {
                return Ok(Some(value));
            }
            Err(error) => return Ok(Some(error.to_string())),
        }
    }
    Ok(None)
}

fn platform_label(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "missing platform metadata".to_string()
    } else {
        value.to_string()
    }
}

fn reject_unsupported_platform(unsupported: Option<String>) -> Result<()> {
    match unsupported {
        Some(platform) => Err(CaseServiceError::UnsupportedPlatform(platform)),
        None => Ok(()),
    }
}
