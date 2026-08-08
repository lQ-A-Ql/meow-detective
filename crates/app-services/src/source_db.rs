use domain::{DataSource, DataSourceId, FileEntryId};
use persistence_sqlite::{
    repositories::datasource_repo::{DataSourceRepo, DataSourceStorage},
    DbError, DbResult,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

mod build;
mod migration;
mod ready;
pub(crate) use build::{
    discard_source_build_db, finalize_source_build_db, open_fresh_source_build_db,
    preserve_unpublished_source_build_db, publish_source_build_db, verify_finalized_source_db,
};
pub use migration::migrate_ready_source_databases;
pub(crate) use ready::open_catalog_recovery_source_by_id;
pub use ready::{
    open_ready_source_by_id, open_ready_source_connections,
    open_ready_source_connections_read_only, open_ready_source_read_only_by_id,
    open_reconstruction_source_by_id, resolve_ready_source_platform, ReadySourceConnection,
    ReadySourceError, ReconstructionSourceConnection, SourceConnectionManager,
};

const SOURCES_DIR_NAME: &str = "sources";
const STAGING_DIR_NAME: &str = "staging";
const SOURCE_DB_FILE_NAME: &str = "source.db";
const SOURCE_INDEX_DIR_NAME: &str = "index";
const CEPH_RECONSTRUCTION_MINIMUM_SOURCE_VERSION: &str = "source_016_file_partition_index";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalFileId {
    pub data_source_id: DataSourceId,
    pub local_id: FileEntryId,
}

impl GlobalFileId {
    pub fn new(data_source_id: DataSourceId, local_id: FileEntryId) -> Self {
        Self {
            data_source_id,
            local_id,
        }
    }

    pub fn encode(&self) -> FileEntryId {
        FileEntryId(encode_source_scoped_id(
            &self.data_source_id,
            &self.local_id.0,
        ))
    }

    pub fn parse(value: &FileEntryId) -> DbResult<Self> {
        let (data_source_id, local_id) = parse_source_scoped_id("File id", &value.0)?;
        Ok(Self::new(data_source_id, FileEntryId(local_id)))
    }
}

pub fn encode_source_scoped_id(data_source_id: &DataSourceId, local_id: &str) -> String {
    format!("ds:{}:{}", data_source_id.0, local_id)
}

pub fn parse_source_scoped_id(label: &str, value: &str) -> DbResult<(DataSourceId, String)> {
    let Some(rest) = value.strip_prefix("ds:") else {
        return Err(DbError::System(format!(
            "{label} '{}' is not a source-scoped id",
            value
        )));
    };
    let Some((data_source_id, local_id)) = rest.split_once(':') else {
        return Err(DbError::System(format!(
            "{label} '{}' is missing source or local id",
            value
        )));
    };
    if data_source_id.is_empty() || local_id.is_empty() {
        return Err(DbError::System(format!(
            "{label} '{}' contains an empty source or local id",
            value
        )));
    }
    if !is_safe_data_source_id(data_source_id) {
        return Err(DbError::System(format!(
            "{label} '{}' contains an invalid source id",
            value
        )));
    }
    Ok((
        DataSourceId(data_source_id.to_string()),
        local_id.to_string(),
    ))
}

fn is_safe_data_source_id(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

pub fn source_dir(case_root: &Path, data_source_id: &DataSourceId) -> PathBuf {
    case_root.join(SOURCES_DIR_NAME).join(&data_source_id.0)
}

pub fn source_db_path(case_root: &Path, data_source_id: &DataSourceId) -> PathBuf {
    source_dir(case_root, data_source_id).join(SOURCE_DB_FILE_NAME)
}

pub(crate) fn canonical_source_db_rel_path(data_source_id: &DataSourceId) -> String {
    format!(
        "{SOURCES_DIR_NAME}/{}/{SOURCE_DB_FILE_NAME}",
        data_source_id.0
    )
}

pub fn source_index_dir(case_root: &Path, data_source_id: &DataSourceId) -> PathBuf {
    source_dir(case_root, data_source_id).join(SOURCE_INDEX_DIR_NAME)
}

pub fn source_content_index_dir(file_index_dir: &Path) -> PathBuf {
    let mut directory_name = file_index_dir
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new(SOURCE_INDEX_DIR_NAME))
        .to_os_string();
    directory_name.push("-content");
    file_index_dir.with_file_name(directory_name)
}

pub fn source_staging_dir(case_root: &Path, data_source_id: &DataSourceId) -> DbResult<PathBuf> {
    if !is_safe_data_source_id(&data_source_id.0) {
        return Err(DbError::System(format!(
            "Data source '{}' cannot own a staging directory",
            data_source_id.0
        )));
    }
    Ok(case_root.join(STAGING_DIR_NAME).join(&data_source_id.0))
}

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
            ready::validate_ready_storage(&source.id, &storage)?;
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

pub fn safe_case_relative_path(case_root: &Path, rel_path: &str) -> DbResult<PathBuf> {
    let rel = Path::new(rel_path);
    if rel.components().any(|component| {
        matches!(
            component,
            std::path::Component::Prefix(_)
                | std::path::Component::RootDir
                | std::path::Component::ParentDir
        )
    }) {
        return Err(DbError::System(format!(
            "Source DB relative path '{}' escapes the case directory",
            rel_path
        )));
    }
    Ok(case_root.join(rel))
}

pub fn safe_existing_case_path(case_root: &Path, path: &Path) -> DbResult<PathBuf> {
    let canonical_root = std::fs::canonicalize(case_root)?;
    let canonical_path = std::fs::canonicalize(path)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(DbError::System(format!(
            "Case-managed path '{}' escapes the case directory '{}'",
            path.display(),
            case_root.display()
        )));
    }
    Ok(canonical_path)
}

fn safe_case_managed_destination(case_root: &Path, path: &Path) -> DbResult<PathBuf> {
    let canonical_root = std::fs::canonicalize(case_root)?;
    let mut ancestor = path;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| {
            DbError::System(format!(
                "Case-managed path '{}' has no existing ancestor",
                path.display()
            ))
        })?;
    }
    let canonical_ancestor = std::fs::canonicalize(ancestor)?;
    if !canonical_ancestor.starts_with(&canonical_root) {
        return Err(DbError::System(format!(
            "Case-managed path '{}' escapes the case directory '{}'",
            path.display(),
            case_root.display()
        )));
    }
    Ok(path.to_path_buf())
}

#[derive(Debug, Clone)]
pub struct SourceDbLocator {
    case_root: PathBuf,
}

impl SourceDbLocator {
    pub fn new(case_root: impl Into<PathBuf>) -> Self {
        Self {
            case_root: case_root.into(),
        }
    }

    pub fn source_dir(&self, data_source_id: &DataSourceId) -> PathBuf {
        source_dir(&self.case_root, data_source_id)
    }

    pub fn source_db_path(&self, data_source_id: &DataSourceId) -> PathBuf {
        source_db_path(&self.case_root, data_source_id)
    }

    pub fn source_index_dir(&self, data_source_id: &DataSourceId) -> PathBuf {
        source_index_dir(&self.case_root, data_source_id)
    }

    pub fn source_staging_dir(&self, data_source_id: &DataSourceId) -> DbResult<PathBuf> {
        source_staging_dir(&self.case_root, data_source_id)
    }
}

#[cfg(test)]
#[path = "../tests/unit/source_db.rs"]
mod tests;
