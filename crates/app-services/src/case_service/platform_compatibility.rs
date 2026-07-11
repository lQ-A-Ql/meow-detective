use super::{CaseServiceError, Result};
use crate::active_case::ActiveCase;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;

pub fn ensure_supported_data_source_platforms(active: &ActiveCase) -> Result<()> {
    let unsupported = active.with_conn(|conn| {
        let repo = DataSourceRepo::new(conn);
        for source in repo.find_by_case(&active.meta.id)? {
            let Some(storage) = repo.find_storage(&source.id)? else {
                continue;
            };
            let platform = storage.platform.trim();
            if !platform.is_empty()
                && !platform.eq_ignore_ascii_case("windows")
                && !platform.eq_ignore_ascii_case("linux")
                && !platform.eq_ignore_ascii_case("unknown")
            {
                return Ok(Some(platform.to_string()));
            }
        }
        Ok(None)
    })?;

    match unsupported {
        Some(platform) => Err(CaseServiceError::UnsupportedPlatform(platform)),
        None => Ok(()),
    }
}
