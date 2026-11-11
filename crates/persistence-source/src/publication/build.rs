//! Source build database lifecycle: open, seal, verify, publish, discard.
//!
//! Every build database is created as `source.db.build.<attempt>` next to the
//! published `source.db`, sealed with the `source.build.finalized` marker, and
//! only then atomically renamed into place.

use crate::control::identity::validate_source_storage_id;
use crate::control::opening::{checkpoint_source_db, verify_source_db_integrity};
use crate::control::paths::{source_db_path, source_dir};
use domain::DataSourceId;
use persistence_sqlite::{DbError, DbResult};
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

const SOURCE_BUILD_DB_FILE_NAME: &str = "source.db.build";
const SOURCE_BUILD_FINALIZED_META_KEY: &str = "source.build.finalized";
const SOURCE_BUILD_FINALIZED_META_VALUE: &str = "v1";
pub const SOURCE_BUILD_WAL_AUTOCHECKPOINT_PAGES: u32 = 16 * 1024;

pub fn source_build_db_path(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
) -> DbResult<PathBuf> {
    validate_source_storage_id(data_source_id)?;
    validate_attempt_id(attempt_id)?;
    Ok(source_dir(case_root, data_source_id)
        .join(format!("{SOURCE_BUILD_DB_FILE_NAME}.{attempt_id}")))
}

pub fn open_fresh_source_build_db(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
) -> DbResult<Connection> {
    validate_source_storage_id(data_source_id)?;
    let final_path = source_db_path(case_root, data_source_id);
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
    if sqlite_file_set_exists(&build_path) {
        return Err(DbError::System(format!(
            "Data source '{}' already has a build database for attempt '{}'",
            data_source_id.0, attempt_id
        )));
    }
    let connection = persistence_sqlite::open_or_create_source(&build_path)?;
    crate::detail::configure_source_identity(&connection, &data_source_id.0)?;
    crate::detail::configure_source_fjall(&connection, &build_path)?;
    connection.pragma_update(
        None,
        "wal_autocheckpoint",
        SOURCE_BUILD_WAL_AUTOCHECKPOINT_PAGES,
    )?;
    Ok(connection)
}

pub fn finalize_source_build_db(conn: &Connection) -> DbResult<()> {
    let source_id = crate::detail::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".to_string()))?;
    let database_path: String = conn.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get(0),
    )?;
    let generation_name: Option<String> = conn
        .query_row(
            "SELECT value FROM source_meta WHERE key = 'fjall_root'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let generation_exists = generation_name.as_deref().is_some_and(|name| {
        Path::new(&database_path)
            .parent()
            .is_some_and(|parent| parent.join(name).is_dir())
    });
    if generation_exists {
        let store = crate::detail::open_store_for_write(conn)?
            .ok_or_else(|| DbError::System("source store is missing".to_string()))?;
        persistence_fjall::verify_source_generation(&store, &source_id)
            .map_err(|error| DbError::System(error.to_string()))?;
    }
    crate::detail::bind_generation(conn, "source.fjall")?;
    verify_source_db_integrity(conn)?;
    checkpoint_source_db(conn)?;
    let journal_mode: String =
        conn.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        return Err(DbError::System(format!(
            "Source build database could not leave WAL mode: {journal_mode}"
        )));
    }
    conn.execute(
        "INSERT INTO source_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![
            SOURCE_BUILD_FINALIZED_META_KEY,
            SOURCE_BUILD_FINALIZED_META_VALUE
        ],
    )?;
    Ok(())
}

pub fn publish_source_build_db(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
) -> DbResult<PathBuf> {
    validate_source_storage_id(data_source_id)?;
    let build_path = source_build_db_path(case_root, data_source_id, attempt_id)?;
    let final_path = source_db_path(case_root, data_source_id);
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
    verify_finalized_source_db(&build_path, data_source_id)?;
    if sqlite_file_set_has_sidecars(&build_path) {
        return Err(DbError::System(format!(
            "Data source '{}' build database still has SQLite sidecars",
            data_source_id.0
        )));
    }
    remove_sqlite_sidecars(&build_path)?;
    let layout = persistence_fjall::SourceBuildLayout::for_case_source(
        case_root,
        &data_source_id.0,
        attempt_id,
    )
    .map_err(|error| DbError::System(error.to_string()))?;
    if layout.build_path().is_dir() {
        let _close_guard = crate::runtime::close_source_store_at_path(&layout.build_path())
            .map_err(|error| DbError::System(error.to_string()))?
            .ok_or_else(|| DbError::System("native source build is still in use".to_string()))?;
        persistence_fjall::publish_source_generation(layout.build_path(), layout.published_path())
            .map_err(|error| DbError::System(error.to_string()))?;
    }
    std::fs::rename(&build_path, &final_path)?;
    Ok(final_path)
}

pub fn preserve_unpublished_source_build_db(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
) -> DbResult<PathBuf> {
    validate_source_storage_id(data_source_id)?;
    publish_source_build_db(case_root, data_source_id, attempt_id)
}

pub fn discard_source_build_db(
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

fn sqlite_file_set_exists(path: &Path) -> bool {
    path.exists() || sqlite_file_set_has_sidecars(path)
}

fn sqlite_file_set_has_sidecars(path: &Path) -> bool {
    let path_text = path.to_string_lossy();
    Path::new(&format!("{path_text}-wal")).exists()
        || Path::new(&format!("{path_text}-shm")).exists()
}

pub fn verify_finalized_source_db(path: &Path, data_source_id: &DataSourceId) -> DbResult<()> {
    let connection = persistence_sqlite::open_existing_source_read_only(path)?;
    let marker = connection
        .query_row(
            "SELECT value FROM source_meta WHERE key = ?1",
            [SOURCE_BUILD_FINALIZED_META_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| {
            DbError::System(format!(
                "Data source '{}' build database is not sealed",
                data_source_id.0
            ))
        })?;
    if marker != SOURCE_BUILD_FINALIZED_META_VALUE {
        return Err(DbError::System(format!(
            "Data source '{}' build database is not sealed",
            data_source_id.0
        )));
    }
    let integrity =
        connection.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))?;
    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(DbError::System(format!(
            "Data source '{}' sealed build database failed integrity check: {integrity}",
            data_source_id.0
        )));
    }
    Ok(())
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
