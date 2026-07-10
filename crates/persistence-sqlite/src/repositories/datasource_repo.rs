use crate::connection::DbResult;
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use rusqlite::{params, Connection};

type ProgressCallback<'a> = &'a dyn Fn(u32, &str);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourceStorage {
    pub storage_model: String,
    pub source_db_rel_path: Option<String>,
    pub index_rel_path: Option<String>,
    pub staging_rel_path: Option<String>,
    pub platform: String,
    pub profile: Option<String>,
    pub import_state: String,
    pub schema_version: Option<String>,
    pub last_error: Option<String>,
}

impl DataSourceStorage {
    pub fn source_db(
        data_source_id: &str,
        platform: Option<&str>,
        profile: Option<String>,
    ) -> Self {
        Self {
            storage_model: "source_db".to_string(),
            source_db_rel_path: Some(format!("sources/{data_source_id}/source.db")),
            index_rel_path: Some(format!("sources/{data_source_id}/index")),
            staging_rel_path: Some(format!("staging/{data_source_id}")),
            platform: platform.unwrap_or("unknown").to_string(),
            profile,
            import_state: "pending".to_string(),
            schema_version: Some(crate::migrations::runner::latest_source_version().to_string()),
            last_error: None,
        }
    }
}

pub struct DataSourceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> DataSourceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, case_id: &CaseId, ds: &DataSource) -> DbResult<()> {
        let storage = DataSourceStorage::source_db(&ds.id.0, None, None);
        self.insert_with_storage(case_id, ds, &storage)
    }

    pub fn insert_with_storage(
        &self,
        case_id: &CaseId,
        ds: &DataSource,
        storage: &DataSourceStorage,
    ) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, source_hash_sha256, hash_status,
                canonical_source_path, evidence_size, reader_kind, provenance_status,
                provenance_warnings, storage_model, source_db_rel_path, index_rel_path,
                staging_rel_path, platform, profile, import_state, schema_version, last_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            params![
                ds.id.0,
                case_id.0,
                ds.name,
                kind_to_str(&ds.kind),
                ds.source_path.display().to_string(),
                ds.provenance.source_hash_sha256.as_deref(),
                hash_status_to_str(&ds.provenance.hash_status),
                ds.provenance
                    .canonical_source_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                ds.provenance.evidence_size.map(|size| size as i64),
                ds.provenance.reader_kind.as_deref(),
                provenance_status_to_str(&ds.provenance.provenance_status),
                serde_json::to_string(&ds.provenance.warnings).unwrap_or_else(|_| "[]".to_string()),
                storage.storage_model,
                storage.source_db_rel_path,
                storage.index_rel_path,
                storage.staging_rel_path,
                storage.platform,
                storage.profile,
                storage.import_state,
                storage.schema_version,
                storage.last_error,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_source_local_metadata(&self, case_id: &CaseId, ds: &DataSource) -> DbResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO data_sources (
                id, case_id, name, kind, source_path, imported_at, source_hash_sha256,
                hash_status, canonical_source_path, evidence_size, reader_kind,
                provenance_status, provenance_warnings
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                ds.id.0,
                case_id.0,
                ds.name,
                kind_to_str(&ds.kind),
                ds.source_path.display().to_string(),
                ds.imported_at.to_rfc3339(),
                ds.provenance.source_hash_sha256.as_deref(),
                hash_status_to_str(&ds.provenance.hash_status),
                ds.provenance
                    .canonical_source_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                ds.provenance.evidence_size.map(|size| size as i64),
                ds.provenance.reader_kind.as_deref(),
                provenance_status_to_str(&ds.provenance.provenance_status),
                serde_json::to_string(&ds.provenance.warnings).unwrap_or_else(|_| "[]".to_string()),
            ],
        )?;
        Ok(())
    }

    pub fn find_by_case(&self, case_id: &CaseId) -> DbResult<Vec<DataSource>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, source_path, imported_at, source_hash_sha256, hash_status,
                canonical_source_path, evidence_size, reader_kind, provenance_status,
                provenance_warnings
             FROM data_sources WHERE case_id = ?1 ORDER BY imported_at DESC, name ASC",
        )?;
        let rows = stmt.query_map(params![case_id.0], |row| {
            Ok(DataSource {
                id: DataSourceId(row.get(0)?),
                name: row.get(1)?,
                kind: str_to_kind(&row.get::<_, String>(2)?),
                source_path: std::path::PathBuf::from(row.get::<_, String>(3)?),
                imported_at: crate::util::parse_datetime(&row.get::<_, String>(4)?),
                provenance: DataSourceProvenance {
                    source_hash_sha256: row.get(5)?,
                    hash_status: str_to_hash_status(row.get::<_, Option<String>>(6)?.as_deref()),
                    canonical_source_path: row
                        .get::<_, Option<String>>(7)?
                        .map(std::path::PathBuf::from),
                    evidence_size: row
                        .get::<_, Option<i64>>(8)?
                        .and_then(|size| u64::try_from(size).ok()),
                    reader_kind: row.get(9)?,
                    provenance_status: str_to_provenance_status(
                        row.get::<_, Option<String>>(10)?.as_deref(),
                    ),
                    warnings: parse_warnings(row.get::<_, Option<String>>(11)?),
                },
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn rename(&self, data_source_id: &DataSourceId, name: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE data_sources SET name = ?1 WHERE id = ?2",
            params![name, data_source_id.0],
        )?;
        Ok(())
    }

    pub fn find_storage(
        &self,
        data_source_id: &DataSourceId,
    ) -> DbResult<Option<DataSourceStorage>> {
        let result = self.conn.query_row(
            "SELECT storage_model, source_db_rel_path, index_rel_path, staging_rel_path,
                    platform, profile, import_state, schema_version, last_error
             FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
            |row| {
                Ok(DataSourceStorage {
                    storage_model: row.get(0)?,
                    source_db_rel_path: row.get(1)?,
                    index_rel_path: row.get(2)?,
                    staging_rel_path: row.get(3)?,
                    platform: row.get(4)?,
                    profile: row.get(5)?,
                    import_state: row.get(6)?,
                    schema_version: row.get(7)?,
                    last_error: row.get(8)?,
                })
            },
        );
        match result {
            Ok(storage) => Ok(Some(storage)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn update_import_state(
        &self,
        data_source_id: &DataSourceId,
        import_state: &str,
        last_error: Option<&str>,
    ) -> DbResult<()> {
        self.conn.execute(
            "UPDATE data_sources
             SET import_state = ?1, last_error = ?2
             WHERE id = ?3",
            params![import_state, last_error, data_source_id.0],
        )?;
        Ok(())
    }

    pub fn update_cluster_membership(
        &self,
        data_source_id: &DataSourceId,
        cluster_id: &str,
        member_index: u32,
        member_count: u32,
    ) -> DbResult<()> {
        let affected = self.conn.execute(
            "UPDATE data_sources
             SET cluster_id = ?1,
                 cluster_member_index = ?2,
                 cluster_member_count = ?3
             WHERE id = ?4",
            params![
                cluster_id,
                i64::from(member_index),
                i64::from(member_count),
                data_source_id.0,
            ],
        )?;
        if affected != 1 {
            return Err(crate::connection::DbError::System(format!(
                "data source not found: {}",
                data_source_id.0
            )));
        }
        Ok(())
    }

    pub fn delete_cascade(&self, data_source_id: &DataSourceId) -> DbResult<()> {
        self.delete_cascade_with_progress(data_source_id, None::<ProgressCallback<'_>>)
    }

    /// Delete data source with cascade and progress callback.
    pub fn delete_cascade_with_progress(
        &self,
        data_source_id: &DataSourceId,
        progress: Option<ProgressCallback<'_>>,
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        if let Some(cb) = progress {
            cb(0, "Deleting data source registration...");
        }
        tx.execute(
            "DELETE FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
        )?;

        tx.commit()?;
        if let Some(cb) = progress {
            cb(100, "Deletion complete");
        }
        Ok(())
    }
}

fn kind_to_str(kind: &DataSourceKind) -> &'static str {
    match kind {
        DataSourceKind::Raw => "raw",
        DataSourceKind::E01 => "e01",
        DataSourceKind::LogicalDirectory => "logical_directory",
    }
}

fn str_to_kind(s: &str) -> DataSourceKind {
    match s {
        "e01" => DataSourceKind::E01,
        "logical_directory" => DataSourceKind::LogicalDirectory,
        _ => DataSourceKind::Raw,
    }
}

fn hash_status_to_str(status: &DataSourceHashStatus) -> &'static str {
    match status {
        DataSourceHashStatus::Unknown => "unknown",
        DataSourceHashStatus::Pending => "pending",
        DataSourceHashStatus::Hashed => "hashed",
        DataSourceHashStatus::Failed => "failed",
        DataSourceHashStatus::Unavailable => "unavailable",
    }
}

fn str_to_hash_status(status: Option<&str>) -> DataSourceHashStatus {
    match status {
        Some("pending") => DataSourceHashStatus::Pending,
        Some("hashed") => DataSourceHashStatus::Hashed,
        Some("failed") => DataSourceHashStatus::Failed,
        Some("unavailable") => DataSourceHashStatus::Unavailable,
        _ => DataSourceHashStatus::Unknown,
    }
}

fn provenance_status_to_str(status: &DataSourceProvenanceStatus) -> &'static str {
    match status {
        DataSourceProvenanceStatus::Unknown => "unknown",
        DataSourceProvenanceStatus::Recorded => "recorded",
        DataSourceProvenanceStatus::Partial => "partial",
        DataSourceProvenanceStatus::Failed => "failed",
    }
}

fn str_to_provenance_status(status: Option<&str>) -> DataSourceProvenanceStatus {
    match status {
        Some("recorded") => DataSourceProvenanceStatus::Recorded,
        Some("partial") => DataSourceProvenanceStatus::Partial,
        Some("failed") => DataSourceProvenanceStatus::Failed,
        _ => DataSourceProvenanceStatus::Unknown,
    }
}

fn parse_warnings(warnings_json: Option<String>) -> Vec<String> {
    warnings_json
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = crate::connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE cases (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                number TEXT,
                examiner TEXT,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE data_sources (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL REFERENCES cases(id),
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
                last_error TEXT,
                cluster_id TEXT,
                cluster_member_index INTEGER,
                cluster_member_count INTEGER
            );
            CREATE TABLE file_entries (
                id TEXT PRIMARY KEY NOT NULL,
                parent_id TEXT,
                data_source_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                size INTEGER,
                ext TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                modified_at TEXT,
                accessed_at TEXT,
                changed_at TEXT,
                hash_sha256 TEXT
            );
            CREATE TABLE artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                data_source_id TEXT NOT NULL DEFAULT '',
                artifact_type TEXT NOT NULL,
                source_object_id TEXT,
                title TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE timeline_events (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                source_object_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                ts TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE data_source_partitions (
                id TEXT PRIMARY KEY,
                data_source_id TEXT NOT NULL,
                partition_index INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind_label TEXT NOT NULL,
                status TEXT NOT NULL,
                type_guid TEXT,
                offset INTEGER NOT NULL,
                length INTEGER NOT NULL,
                filesystem TEXT,
                unlock_hint TEXT,
                lvm_vg_uuid TEXT,
                lvm_vg_name TEXT,
                lvm_lv_uuid TEXT,
                lvm_lv_name TEXT,
                lvm_pv_offsets_json TEXT,
                lvm_pv_sources_json TEXT
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
            params!["case-1", "Test Case"],
        ).unwrap();
        conn
    }

    fn make_ds(id: &str, name: &str) -> DataSource {
        DataSource {
            id: DataSourceId(id.to_string()),
            name: name.to_string(),
            kind: DataSourceKind::Raw,
            source_path: std::path::PathBuf::from("/evidence/image.E01"),
            imported_at: chrono::Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        }
    }

    #[test]
    fn insert_then_find_by_case_returns_it() {
        let conn = setup_db();
        let repo = DataSourceRepo::new(&conn);
        let ds = make_ds("ds-1", "Disk Image");
        repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

        let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Disk Image");
        assert_eq!(results[0].kind, DataSourceKind::Raw);
    }

    #[test]
    fn insert_then_find_by_case_round_trips_provenance() {
        let conn = setup_db();
        let repo = DataSourceRepo::new(&conn);
        let mut ds = make_ds("ds-1", "Disk Image");
        ds.provenance = DataSourceProvenance {
            source_hash_sha256: Some("a".repeat(64)),
            hash_status: DataSourceHashStatus::Hashed,
            canonical_source_path: Some(std::path::PathBuf::from("/canonical/image.E01")),
            evidence_size: Some(42_000),
            reader_kind: Some("raw-image".to_string()),
            provenance_status: DataSourceProvenanceStatus::Recorded,
            warnings: vec![
                "sparse image metadata".to_string(),
                "hash verified".to_string(),
            ],
        };

        repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

        let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provenance, ds.provenance);
    }

    #[test]
    fn legacy_null_provenance_loads_safe_defaults() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, imported_at, source_hash_sha256,
                hash_status, canonical_source_path, evidence_size, reader_kind,
                provenance_status, provenance_warnings
            ) VALUES (
                'legacy-ds', 'case-1', 'Legacy', 'raw', '/legacy.raw',
                '2026-01-01T00:00:00Z', NULL, NULL, NULL, NULL, NULL, NULL, NULL
            )",
            [],
        )
        .unwrap();

        let results = DataSourceRepo::new(&conn)
            .find_by_case(&CaseId("case-1".to_string()))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provenance, DataSourceProvenance::unknown());
    }

    #[test]
    fn rename_changes_the_name() {
        let conn = setup_db();
        let repo = DataSourceRepo::new(&conn);
        let ds = make_ds("ds-1", "Old Name");
        repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

        repo.rename(&DataSourceId("ds-1".to_string()), "New Name")
            .unwrap();

        let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
        assert_eq!(results[0].name, "New Name");
    }

    #[test]
    fn update_cluster_membership_requires_existing_data_source() {
        let conn = setup_db();
        let repo = DataSourceRepo::new(&conn);
        let ds = make_ds("ds-1", "Disk Image");
        repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

        repo.update_cluster_membership(&DataSourceId("ds-1".to_string()), "cluster-1", 1, 3)
            .unwrap();

        let stored: (String, i64, i64) = conn
            .query_row(
                "SELECT cluster_id, cluster_member_index, cluster_member_count
                 FROM data_sources WHERE id = 'ds-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(stored, ("cluster-1".to_string(), 1, 3));

        let error =
            repo.update_cluster_membership(&DataSourceId("missing".to_string()), "cluster-1", 0, 3);
        assert!(error.is_err());
    }

    #[test]
    fn delete_cascade_removes_the_record() {
        let conn = setup_db();
        let repo = DataSourceRepo::new(&conn);
        let ds = make_ds("ds-1", "Disk Image");
        repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

        repo.delete_cascade(&DataSourceId("ds-1".to_string()))
            .unwrap();

        let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
        assert!(results.is_empty());
    }
}
