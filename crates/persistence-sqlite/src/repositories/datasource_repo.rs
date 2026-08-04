use crate::connection::DbResult;
use crate::repositories::audit_repo::{AuditAction, AuditRepo};
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use rusqlite::{params, Connection, OptionalExtension};

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

    pub fn find_ids_by_cluster(
        &self,
        case_id: &CaseId,
        cluster_id: &str,
    ) -> DbResult<Vec<DataSourceId>> {
        let mut statement = self.conn.prepare(
            "SELECT id
             FROM data_sources
             WHERE case_id = ?1 AND cluster_id = ?2
             ORDER BY cluster_member_index ASC, id ASC",
        )?;
        let rows = statement.query_map(params![case_id.0, cluster_id], |row| {
            Ok(DataSourceId(row.get(0)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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

    pub fn source_path(&self, data_source_id: &DataSourceId) -> DbResult<String> {
        Ok(self.conn.query_row(
            "SELECT source_path FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
            |row| row.get(0),
        )?)
    }

    pub fn source_fingerprint(&self, data_source_id: &DataSourceId) -> DbResult<Option<String>> {
        Ok(self.conn.query_row(
            "SELECT source_hash_sha256 FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
            |row| row.get(0),
        )?)
    }

    pub fn source_evidence_size(&self, data_source_id: &DataSourceId) -> DbResult<Option<u64>> {
        let size = self.conn.query_row(
            "SELECT evidence_size FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        Ok(size.and_then(|value| u64::try_from(value).ok()))
    }

    pub fn source_kind(&self, data_source_id: &DataSourceId) -> DbResult<DataSourceKind> {
        Ok(self.conn.query_row(
            "SELECT kind FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
            |row| row.get::<_, String>(0).map(|kind| str_to_kind(&kind)),
        )?)
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

    pub fn update_schema_version(
        &self,
        data_source_id: &DataSourceId,
        schema_version: &str,
    ) -> DbResult<()> {
        self.conn.execute(
            "UPDATE data_sources SET schema_version = ?1 WHERE id = ?2",
            params![schema_version, data_source_id.0],
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
             WHERE id = ?4
               AND EXISTS (
                   SELECT 1
                   FROM data_source_clusters AS cluster
                   WHERE cluster.id = ?1
                     AND cluster.case_id = data_sources.case_id
               )",
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

    pub fn delete_cascade_with_audit(
        &self,
        data_source_id: &DataSourceId,
        details: &str,
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        let case_id = delete_registered_data_source(&tx, data_source_id)?;
        AuditRepo::new(&tx).log(
            Some(&case_id),
            "system",
            &AuditAction::DataSourceDelete,
            Some(&data_source_id.0),
            details,
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn delete_registered_data_source(
    conn: &Connection,
    data_source_id: &DataSourceId,
) -> DbResult<String> {
    conn.query_row(
        "DELETE FROM data_sources WHERE id = ?1 RETURNING case_id",
        params![data_source_id.0],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .ok_or_else(|| {
        crate::connection::DbError::System(format!("data source not found: {}", data_source_id.0))
    })
}

fn kind_to_str(kind: &DataSourceKind) -> &'static str {
    match kind {
        DataSourceKind::Raw => "raw",
        DataSourceKind::E01 => "e01",
        DataSourceKind::LogicalDirectory => "logical_directory",
        DataSourceKind::CephRbd => "ceph_rbd",
        DataSourceKind::CephFs => "ceph_fs",
    }
}

fn str_to_kind(s: &str) -> DataSourceKind {
    match s {
        "e01" => DataSourceKind::E01,
        "logical_directory" => DataSourceKind::LogicalDirectory,
        "ceph_rbd" => DataSourceKind::CephRbd,
        "ceph_fs" => DataSourceKind::CephFs,
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
#[path = "../../tests/unit/repositories/datasource_repo.rs"]
mod tests;
