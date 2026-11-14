//! Resolve the active native generation from the source control database.

use super::identity::validate_source_storage_id;
use super::paths::source_db_path;
use domain::DataSourceId;
use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub fn current_source_generation_root(
    case_root: &Path,
    source_id: &DataSourceId,
) -> DbResult<PathBuf> {
    let (_, generation) = open_current_source_generation(case_root, source_id)?;
    Ok(generation)
}

pub(crate) fn open_current_source_generation(
    case_root: &Path,
    source_id: &DataSourceId,
) -> DbResult<(Connection, PathBuf)> {
    validate_source_storage_id(source_id)?;
    let connection =
        super::opening::open_source_database_read_only(&source_db_path(case_root, source_id))?;
    let generation = bound_source_generation_root(&connection, source_id)?;
    let case = std::fs::canonicalize(case_root)?;
    let source = std::fs::canonicalize(
        source_db_path(case_root, source_id)
            .parent()
            .ok_or_else(|| DbError::System("source database has no parent".into()))?,
    )?;
    let actual = std::fs::canonicalize(&generation)?;
    if !source.starts_with(&case) || actual.parent() != Some(source.as_path()) {
        return Err(DbError::System(
            "source generation is outside its source directory".into(),
        ));
    }
    persistence_fjall::read_source_generation_manifest_at_path(&actual, &source_id.0)
        .map_err(|error| DbError::System(error.to_string()))?;
    Ok((connection, generation))
}

pub fn bound_source_generation_root(
    connection: &Connection,
    expected_source: &DataSourceId,
) -> DbResult<PathBuf> {
    validate_source_storage_id(expected_source)?;
    crate::detail::require_owner(connection, &expected_source.0)?;
    let name: String = connection.query_row(
        "SELECT value FROM source_meta WHERE key = 'fjall_root'",
        [],
        |row| row.get(0),
    )?;
    validate_generation_name(&name)?;
    let database_path: String = connection.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get(0),
    )?;
    let parent = Path::new(&database_path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| DbError::System("source control database has no directory".into()))?;
    Ok(parent.join(name))
}

pub(crate) fn validate_generation_name(name: &str) -> DbResult<()> {
    if (name != "source.fjall" && !name.starts_with("source.fjall.build."))
        || name.contains(['/', '\\', ':', '\0'])
        || name.ends_with('.')
    {
        return Err(DbError::System("invalid source generation name".into()));
    }
    Ok(())
}
