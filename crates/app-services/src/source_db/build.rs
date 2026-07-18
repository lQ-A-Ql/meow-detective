use domain::DataSourceId;
use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const SOURCE_BUILD_DB_FILE_NAME: &str = "source.db.build";
pub(super) const SOURCE_BUILD_WAL_AUTOCHECKPOINT_PAGES: u32 = 16 * 1024;

pub(crate) fn source_build_db_path(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
) -> DbResult<PathBuf> {
    validate_attempt_id(attempt_id)?;
    Ok(super::source_dir(case_root, data_source_id)
        .join(format!("{SOURCE_BUILD_DB_FILE_NAME}.{attempt_id}")))
}

pub(crate) fn open_fresh_source_build_db(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
) -> DbResult<Connection> {
    let final_path = super::source_db_path(case_root, data_source_id);
    if final_path.exists() {
        return Err(DbError::System(format!(
            "Data source '{}' already has a published source database",
            data_source_id.0
        )));
    }
    let build_path = source_build_db_path(case_root, data_source_id, attempt_id)?;
    if let Some(parent) = build_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    remove_sqlite_file_set(&build_path)?;
    let connection = persistence_sqlite::open_or_create_source(&build_path)?;
    connection.pragma_update(
        None,
        "wal_autocheckpoint",
        SOURCE_BUILD_WAL_AUTOCHECKPOINT_PAGES,
    )?;
    Ok(connection)
}

pub(crate) fn finalize_source_build_db(conn: &Connection) -> DbResult<()> {
    super::verify_source_db_integrity(conn)?;
    super::checkpoint_source_db(conn)?;
    let journal_mode: String =
        conn.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        return Err(DbError::System(format!(
            "Source build database could not leave WAL mode: {journal_mode}"
        )));
    }
    Ok(())
}

pub(crate) fn publish_source_build_db(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
) -> DbResult<PathBuf> {
    let build_path = source_build_db_path(case_root, data_source_id, attempt_id)?;
    let final_path = super::source_db_path(case_root, data_source_id);
    if !build_path.is_file() {
        return Err(DbError::System(format!(
            "Data source '{}' build database is missing",
            data_source_id.0
        )));
    }
    if final_path.exists() {
        return Err(DbError::System(format!(
            "Data source '{}' published database already exists",
            data_source_id.0
        )));
    }
    remove_sqlite_sidecars(&build_path)?;
    std::fs::rename(&build_path, &final_path)?;
    Ok(final_path)
}

pub(crate) fn discard_source_build_db(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
) -> DbResult<()> {
    remove_sqlite_file_set(&source_build_db_path(
        case_root,
        data_source_id,
        attempt_id,
    )?)
}

fn validate_attempt_id(attempt_id: &str) -> DbResult<()> {
    uuid::Uuid::parse_str(attempt_id)
        .map(|_| ())
        .map_err(|_| DbError::System("Source build attempt ID is invalid".to_string()))
}

fn remove_sqlite_file_set(path: &Path) -> DbResult<()> {
    remove_file_if_present(path)?;
    remove_sqlite_sidecars(path)
}

fn remove_sqlite_sidecars(path: &Path) -> DbResult<()> {
    let path_text = path.to_string_lossy();
    remove_file_if_present(Path::new(&format!("{path_text}-wal")))?;
    remove_file_if_present(Path::new(&format!("{path_text}-shm")))
}

fn remove_file_if_present(path: &Path) -> DbResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
