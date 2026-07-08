use domain::{DataSourceId, FileEntryId};
use persistence_sqlite::{repositories::datasource_repo::DataSourceRepo, DbError, DbResult};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

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
    Ok((
        DataSourceId(data_source_id.to_string()),
        local_id.to_string(),
    ))
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
    persistence_sqlite::open_existing_source(&db_path)
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

    pub fn open_registered(
        &self,
        case_conn: &Connection,
        data_source_id: &DataSourceId,
    ) -> DbResult<Connection> {
        open_registered_source_db(case_conn, &self.locator.case_root, data_source_id)
    }

    pub fn open_for_global_file_id(
        &self,
        case_conn: &Connection,
        file_id: &FileEntryId,
    ) -> DbResult<(GlobalFileId, Connection)> {
        let global_id = GlobalFileId::parse(file_id)?;
        let conn = self.open_registered(case_conn, &global_id.data_source_id)?;
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
mod tests {
    use super::*;
    use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};

    #[test]
    fn global_file_id_round_trips() {
        let global = GlobalFileId::new(
            DataSourceId("ds-1".to_string()),
            FileEntryId("mft:0:42".to_string()),
        );

        let encoded = global.encode();
        assert_eq!(encoded.0, "ds:ds-1:mft:0:42");
        assert_eq!(GlobalFileId::parse(&encoded).unwrap(), global);
    }

    #[test]
    fn global_file_id_rejects_unscoped_ids() {
        let err = GlobalFileId::parse(&FileEntryId("mft:0:42".to_string())).unwrap_err();

        assert!(err.to_string().contains("not a source-scoped id"));
    }

    #[test]
    fn safe_case_relative_path_rejects_escape_paths() {
        let case_root = Path::new("D:/cases/case-1");

        for rel_path in ["../outside/source.db", "/tmp/source.db", "C:/tmp/source.db"] {
            let err = safe_case_relative_path(case_root, rel_path).unwrap_err();
            assert!(err.to_string().contains("escapes the case directory"));
        }

        assert_eq!(
            safe_case_relative_path(case_root, "sources/ds-1/source.db").unwrap(),
            case_root.join("sources/ds-1/source.db")
        );
    }

    #[test]
    fn open_registered_source_db_rejects_missing_source_db() {
        let tmp = tempfile::TempDir::new().unwrap();
        let case_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        case_conn
            .execute_batch(
                "CREATE TABLE data_sources (
                    id TEXT PRIMARY KEY NOT NULL,
                    case_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    imported_at TEXT NOT NULL DEFAULT (datetime('now')),
                    source_hash_sha256 TEXT,
                    hash_status TEXT DEFAULT 'unknown',
                    canonical_source_path TEXT,
                    evidence_size INTEGER,
                    reader_kind TEXT,
                    provenance_status TEXT DEFAULT 'unknown',
                    provenance_warnings TEXT DEFAULT '[]',
                    storage_model TEXT NOT NULL DEFAULT 'source_db',
                    source_db_rel_path TEXT,
                    index_rel_path TEXT,
                    staging_rel_path TEXT,
                    platform TEXT NOT NULL DEFAULT 'unknown',
                    profile TEXT,
                    import_state TEXT NOT NULL DEFAULT 'pending',
                    schema_version TEXT,
                    last_error TEXT
                );",
            )
            .unwrap();
        let ds = domain::DataSource {
            id: DataSourceId("ds-missing".to_string()),
            name: "Missing Source".to_string(),
            kind: domain::DataSourceKind::Raw,
            source_path: std::path::PathBuf::from("D:/missing.raw"),
            imported_at: chrono::Utc::now(),
            provenance: domain::DataSourceProvenance::unknown(),
        };
        DataSourceRepo::new(&case_conn)
            .insert_with_storage(
                &domain::CaseId("case-1".to_string()),
                &ds,
                &DataSourceStorage::source_db(&ds.id.0, Some("linux"), None),
            )
            .unwrap();

        let err = open_registered_source_db(&case_conn, tmp.path(), &ds.id).unwrap_err();

        assert!(err.to_string().contains("source DB is missing"));
    }
}
