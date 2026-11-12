//! Source database opening facade.
//!
//! Opening, schema verification, and WAL durability live in
//! `persistence-source`; this module keeps the existing
//! `crate::source_db::opening::*` paths plus the ready-state enumeration that
//! depends on the application-level ready-source policy.

pub use persistence_source::control::opening::*;

use domain::DataSource;
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use rusqlite::Connection;

use super::ready::{validate_ready_storage, ReadySourceError};

pub fn ready_data_sources(
    case_conn: &Connection,
    case_id: &domain::CaseId,
) -> Result<Vec<(DataSource, DataSourceStorage)>, ReadySourceError> {
    let repo = DataSourceRepo::new(case_conn);
    let mut ready = Vec::new();
    for source in repo.find_by_case(case_id)? {
        let storage = repo
            .find_storage(&source.id)?
            .ok_or_else(|| ReadySourceError::NotFound {
                case_id: case_id.0.clone(),
                data_source_id: source.id.0.clone(),
            })?;
        if storage.import_state.trim().eq_ignore_ascii_case("ready") {
            validate_ready_storage(&source.id, &storage)?;
            ready.push((source, storage));
        }
    }
    Ok(ready)
}
