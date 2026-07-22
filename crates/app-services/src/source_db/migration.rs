use super::open_registered_source_db;
use domain::CaseId;
use persistence_sqlite::{repositories::datasource_repo::DataSourceRepo, DbResult};
use rusqlite::Connection;
use std::path::Path;

/// Migrate ready source databases before exposing them to case reads.
/// Read-only consumers intentionally do not run migrations, so case opening
/// is the safe point for repairing derived catalog metadata.
pub fn migrate_ready_source_databases(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> DbResult<()> {
    let repo = DataSourceRepo::new(case_conn);
    for source in repo.find_by_case(case_id)? {
        let Some(storage) = repo.find_storage(&source.id)? else {
            continue;
        };
        if matches!(
            storage.import_state.trim().to_ascii_lowercase().as_str(),
            "ready" | "ready_metadata"
        ) {
            let _connection = open_registered_source_db(case_conn, case_root, &source.id)?;
        }
    }
    Ok(())
}
