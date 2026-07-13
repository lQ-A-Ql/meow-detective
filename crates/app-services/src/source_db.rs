use domain::{DataSource, DataSourceId, FileEntryId};
use persistence_sqlite::{
    repositories::datasource_repo::{DataSourceRepo, DataSourceStorage},
    DbError, DbResult,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

mod ready;
pub use ready::{
    open_ready_source_by_id, resolve_ready_source_platform, ReadySourceConnection, ReadySourceError,
};

const SOURCES_DIR_NAME: &str = "sources";
const SOURCE_DB_FILE_NAME: &str = "source.db";
const SOURCE_INDEX_DIR_NAME: &str = "index";

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

pub fn source_index_dir(case_root: &Path, data_source_id: &DataSourceId) -> PathBuf {
    source_dir(case_root, data_source_id).join(SOURCE_INDEX_DIR_NAME)
}

pub fn open_source_db(case_root: &Path, data_source_id: &DataSourceId) -> DbResult<Connection> {
    persistence_sqlite::open_or_create_source(&source_db_path(case_root, data_source_id))
}

pub fn open_registered_source_db(
    case_conn: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> DbResult<Connection> {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)?
        .ok_or_else(|| DbError::System(format!("Data source '{}' not found", data_source_id.0)))?;
    if storage.storage_model != "source_db" {
        return Err(DbError::System(format!(
            "Data source '{}' uses unsupported storage model '{}'",
            data_source_id.0, storage.storage_model
        )));
    }
    let expected_schema_version =
        persistence_sqlite::migrations::runner::latest_source_version().to_string();
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
    let db_path = safe_case_relative_path(case_root, &rel_path)?;
    if !db_path.exists() {
        return Err(DbError::System(format!(
            "Data source '{}' source DB is missing at {}; re-import is required",
            data_source_id.0,
            db_path.display()
        )));
    }
    let db_path = safe_existing_case_path(case_root, &db_path)?;
    let connection = persistence_sqlite::open_existing_source(&db_path)?;
    if storage.schema_version.as_deref() != Some(expected_schema_version.as_str()) {
        DataSourceRepo::new(case_conn)
            .update_schema_version(data_source_id, &expected_schema_version)?;
    }
    Ok(connection)
}

/// Open only fully imported source databases for case-wide aggregation.
pub fn open_ready_source_connections(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<Vec<(DataSourceId, Connection)>, ReadySourceError> {
    let sources = ready_data_sources(case_conn, case_id)?;
    let mut connections = Vec::with_capacity(sources.len());
    for (source, _) in sources {
        let ready = open_ready_source_by_id(case_conn, case_root, case_id, &source.id)?;
        connections.push((ready.data_source_id, ready.connection));
    }
    Ok(connections)
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
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
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
}

#[derive(Debug, Clone)]
pub struct SourceConnectionManager {
    locator: SourceDbLocator,
}

impl SourceConnectionManager {
    pub fn new(case_root: impl Into<PathBuf>) -> Self {
        Self {
            locator: SourceDbLocator::new(case_root),
        }
    }

    pub fn open_ready(
        &self,
        case_conn: &Connection,
        case_id: &domain::CaseId,
        data_source_id: &DataSourceId,
    ) -> Result<Connection, ReadySourceError> {
        Ok(
            open_ready_source_by_id(case_conn, &self.locator.case_root, case_id, data_source_id)?
                .connection,
        )
    }

    pub fn open_ready_for_global_file_id(
        &self,
        case_conn: &Connection,
        case_id: &domain::CaseId,
        file_id: &FileEntryId,
    ) -> Result<(GlobalFileId, Connection), ReadySourceError> {
        let global_id = GlobalFileId::parse(file_id)?;
        let conn = self.open_ready(case_conn, case_id, &global_id.data_source_id)?;
        Ok((global_id, conn))
    }
}

pub fn wrap_file_entry_id(entry: &mut domain::FileEntry) {
    if entry.id.0.starts_with("ds:") {
        return;
    }
    let data_source_id = entry.data_source_id.clone();
    entry.id = GlobalFileId::new(data_source_id.clone(), entry.id.clone()).encode();
    if let Some(parent_id) = entry.parent_id.clone() {
        entry.parent_id = if parent_id.0.starts_with("ds:") {
            Some(parent_id)
        } else {
            Some(GlobalFileId::new(data_source_id, parent_id).encode())
        };
    }
}

pub fn wrap_file_entry_ids(entries: &mut [domain::FileEntry]) {
    for entry in entries {
        wrap_file_entry_id(entry);
    }
}

#[cfg(test)]
#[path = "../tests/unit/source_db.rs"]
mod tests;
