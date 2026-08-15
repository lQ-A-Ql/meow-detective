use super::{
    path_safety::{
        safe_case_managed_destination, safe_case_relative_path, safe_existing_case_path,
    },
    paths::{canonical_source_db_rel_path, source_db_path},
    ready::{validate_ready_storage, ReadySourceError},
};
use domain::{DataSource, DataSourceId};
use persistence_sqlite::{
    repositories::datasource_repo::{DataSourceRepo, DataSourceStorage},
    DbError, DbResult,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const CEPH_RECONSTRUCTION_MINIMUM_SOURCE_VERSION: &str = "source_016_file_partition_index";

pub fn open_source_db(case_root: &Path, data_source_id: &DataSourceId) -> DbResult<Connection> {
    persistence_sqlite::open_or_create_source(&source_db_path(case_root, data_source_id))
}

pub fn open_registered_source_db(
    case_conn: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> DbResult<Connection> {
    let db_path = registered_source_db_path(case_conn, case_root, data_source_id)?;
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)?
        .ok_or_else(|| DbError::System(format!("Data source '{}' not found", data_source_id.0)))?;
    let expected_schema_version =
        persistence_sqlite::migrations::runner::latest_source_version().to_string();
    let connection = persistence_sqlite::open_existing_source(&db_path)?;
    if storage.schema_version.as_deref() != Some(expected_schema_version.as_str()) {
        DataSourceRepo::new(case_conn)
            .update_schema_version(data_source_id, &expected_schema_version)?;
    }
    Ok(connection)
}

pub fn open_registered_source_db_read_only(
    case_conn: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> DbResult<Connection> {
    open_registered_source_db_read_only_at_least(
        case_conn,
        case_root,
        data_source_id,
        persistence_sqlite::migrations::runner::latest_source_version(),
    )
}

pub(crate) fn open_registered_reconstruction_source_db_read_only(
    case_conn: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> DbResult<Connection> {
    open_registered_source_db_read_only_at_least(
        case_conn,
        case_root,
        data_source_id,
        CEPH_RECONSTRUCTION_MINIMUM_SOURCE_VERSION,
    )
}

fn open_registered_source_db_read_only_at_least(
    case_conn: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    minimum_schema_version: &str,
) -> DbResult<Connection> {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)?
        .ok_or_else(|| DbError::System(format!("Data source '{}' not found", data_source_id.0)))?;
    let registered_schema_version = storage.schema_version.as_deref().ok_or_else(|| {
        DbError::System(format!(
            "Data source '{}' is missing source DB schema version",
            data_source_id.0
        ))
    })?;
    if !persistence_sqlite::migrations::runner::source_version_is_at_least(
        registered_schema_version,
        minimum_schema_version,
    ) {
        return Err(DbError::System(format!(
            "Data source '{}' requires source DB migration to at least '{}' before read-only access",
            data_source_id.0, minimum_schema_version
        )));
    }
    let db_path = registered_source_db_path(case_conn, case_root, data_source_id)?;
    let connection = persistence_sqlite::open_existing_source_read_only(&db_path)?;
    let actual_schema_version =
        persistence_sqlite::migrations::runner::current_version(&connection)?;
    let actual_schema_version = actual_schema_version.as_deref().ok_or_else(|| {
        DbError::System(format!(
            "Data source '{}' physical source DB has no schema version",
            data_source_id.0
        ))
    })?;
    if actual_schema_version != registered_schema_version {
        return Err(DbError::System(format!(
            "Data source '{}' source DB schema metadata is inconsistent; registered '{}', physical '{}'",
            data_source_id.0, registered_schema_version, actual_schema_version
        )));
    }
    if !persistence_sqlite::migrations::runner::source_version_is_at_least(
        actual_schema_version,
        minimum_schema_version,
    ) {
        return Err(DbError::System(format!(
            "Data source '{}' physical source DB schema is stale; requires at least '{}', found '{}'",
            data_source_id.0,
            minimum_schema_version,
            actual_schema_version
        )));
    }
    Ok(connection)
}

pub fn registered_source_db_path(
    case_conn: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> DbResult<PathBuf> {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)?
        .ok_or_else(|| DbError::System(format!("Data source '{}' not found", data_source_id.0)))?;
    if storage.storage_model != "source_db" {
        return Err(DbError::System(format!(
            "Data source '{}' uses unsupported storage model '{}'",
            data_source_id.0, storage.storage_model
        )));
    }
    if storage.schema_version.is_none() {
        return Err(DbError::System(format!(
            "Data source '{}' is missing source DB schema version; re-import is required",
            data_source_id.0
        )));
    }
    let rel_path = storage.source_db_rel_path.ok_or_else(|| {
        DbError::System(format!(
            "Data source '{}' is missing source DB path; re-import is required",
            data_source_id.0
        ))
    })?;
    if rel_path.replace('\\', "/") != canonical_source_db_rel_path(data_source_id) {
        return Err(DbError::System(format!(
            "Data source '{}' is bound to a non-canonical source DB path; re-import is required",
            data_source_id.0
        )));
    }
    let db_path = safe_case_relative_path(case_root, &rel_path)?;
    if !db_path.exists() {
        return Err(DbError::System(format!(
            "Data source '{}' source DB is missing at {}; re-import is required",
            data_source_id.0,
            db_path.display()
        )));
    }
    safe_existing_case_path(case_root, &db_path)
}

pub fn registered_source_index_dir(
    case_conn: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> DbResult<PathBuf> {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)?
        .ok_or_else(|| DbError::System(format!("Data source '{}' not found", data_source_id.0)))?;
    if storage.storage_model != "source_db" {
        return Err(DbError::System(format!(
            "Data source '{}' uses unsupported storage model '{}'",
            data_source_id.0, storage.storage_model
        )));
    }
    let rel_path = storage.index_rel_path.ok_or_else(|| {
        DbError::System(format!(
            "Data source '{}' is missing search index path; re-import is required",
            data_source_id.0
        ))
    })?;
    let index_dir = safe_case_relative_path(case_root, &rel_path)?;
    safe_case_managed_destination(case_root, &index_dir)
}

pub(crate) fn ready_data_sources(
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

pub fn checkpoint_source_db(conn: &Connection) -> DbResult<()> {
    let (busy, log_frames, checkpointed_frames): (u32, u64, u64) =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    if busy != 0 || log_frames != checkpointed_frames {
        return Err(DbError::System(format!(
            "Source DB WAL checkpoint did not converge: busy={busy}, logFrames={log_frames}, checkpointedFrames={checkpointed_frames}"
        )));
    }
    Ok(())
}

pub fn verify_source_db_integrity(conn: &Connection) -> DbResult<()> {
    let result = conn.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))?;
    if result.eq_ignore_ascii_case("ok") {
        return Ok(());
    }
    Err(DbError::System(format!(
        "Source DB integrity check failed: {result}"
    )))
}
