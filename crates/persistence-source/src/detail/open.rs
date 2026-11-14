use persistence_fjall::{DatabaseConfig, SourceStore};
use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;
use std::sync::Arc;

const FJALL_ROOT_KEY: &str = "fjall_root";
const SOURCE_ID_KEY: &str = "data_source_id";

pub fn configure_source_fjall(conn: &Connection, source_db: &std::path::Path) -> DbResult<()> {
    let file_name = source_db
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let root_name = file_name
        .strip_prefix("source.db.build.")
        .map(|attempt| format!("source.fjall.build.{attempt}"))
        .unwrap_or_else(|| "source.fjall".to_owned());
    conn.execute(
        "INSERT INTO source_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO NOTHING",
        rusqlite::params![FJALL_ROOT_KEY, root_name],
    )?;
    Ok(())
}

pub fn configure_source_identity(conn: &Connection, source_id: &str) -> DbResult<()> {
    if let Some(owner) = self::source_id(conn)? {
        if owner != source_id {
            return Err(DbError::System("source identity cannot be replaced".into()));
        }
        return Ok(());
    }
    conn.execute(
        "INSERT INTO source_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![SOURCE_ID_KEY, source_id],
    )?;
    Ok(())
}

pub fn bind_generation(conn: &Connection, name: &str) -> DbResult<()> {
    crate::control::generation::validate_generation_name(name)?;
    let changed = conn.execute(
        "UPDATE source_meta SET value = ?1 WHERE key = ?2",
        rusqlite::params![name, FJALL_ROOT_KEY],
    )?;
    if changed != 1 {
        return Err(DbError::System(
            "source generation binding is missing".into(),
        ));
    }
    Ok(())
}

pub fn source_id(conn: &Connection) -> DbResult<Option<String>> {
    match conn.query_row(
        "SELECT value FROM source_meta WHERE key = ?1",
        [SOURCE_ID_KEY],
        |row| row.get(0),
    ) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => {
            let missing_table = matches!(
                &error,
                rusqlite::Error::SqliteFailure(code, message)
                    if code.extended_code == rusqlite::ffi::SQLITE_ERROR
                        && message.as_deref().is_some_and(|value| value.contains("no such table"))
            );
            if missing_table {
                Ok(None)
            } else {
                Err(error.into())
            }
        }
    }
}

pub(crate) fn require_owner(conn: &Connection, requested: &str) -> DbResult<()> {
    if source_id(conn)?.as_deref() != Some(requested) {
        return Err(DbError::System("source detail owner mismatch".into()));
    }
    Ok(())
}

pub fn require_source_store(conn: &Connection) -> DbResult<Arc<SourceStore>> {
    if source_id(conn)?.is_none() {
        return Err(DbError::System("source identity is missing".into()));
    }
    open_store(conn)?.ok_or_else(|| DbError::System("source detail storage is missing".into()))
}

pub fn open_store(conn: &Connection) -> DbResult<Option<Arc<SourceStore>>> {
    open_store_with_policy(conn, false)
}

pub fn open_store_for_write(conn: &Connection) -> DbResult<Option<Arc<SourceStore>>> {
    let query_only: bool = conn.pragma_query_value(None, "query_only", |row| row.get(0))?;
    if query_only || conn.is_readonly("main")? {
        return Err(DbError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "source connection is read-only",
        )));
    }
    open_store_with_policy(conn, true)
}

fn open_store_with_policy(
    conn: &Connection,
    create_missing: bool,
) -> DbResult<Option<Arc<SourceStore>>> {
    let root: Option<String> = match conn.query_row(
        "SELECT value FROM source_meta WHERE key = ?1",
        [FJALL_ROOT_KEY],
        |row| row.get(0),
    ) {
        Ok(value) => Some(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => {
            let missing_table = matches!(
                &error,
                rusqlite::Error::SqliteFailure(code, message)
                    if code.extended_code == rusqlite::ffi::SQLITE_ERROR
                        && message.as_deref().is_some_and(|value| value.contains("no such table"))
            );
            if missing_table {
                None
            } else {
                return Err(error.into());
            }
        }
    };
    let Some(name) = root else {
        if source_id(conn)?.is_some() {
            return Err(DbError::System("source detail storage is missing".into()));
        }
        return Ok(None);
    };
    crate::control::generation::validate_generation_name(&name)?;
    let database_path: String = conn.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get(0),
    )?;
    let parent = std::path::Path::new(&database_path)
        .parent()
        .ok_or_else(|| DbError::System("source database has no directory".into()))?;
    let path = parent.join(name);
    if !path.is_dir() {
        if !create_missing {
            return Err(DbError::System(
                "source generation directory is missing".into(),
            ));
        }
        std::fs::create_dir_all(&path)?;
    }
    persistence_fjall::open_shared_source_store(path, DatabaseConfig::default())
        .map(Some)
        .map_err(|error| DbError::System(error.to_string()))
}
