use persistence_sqlite::connection::{DbError, DbResult};
use rusqlite::{params, Connection};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfrastructureNetworkFactRecord {
    pub id: String,
    pub case_id: String,
    pub data_source_id: String,
    pub environment_object_id: String,
    pub file_id: String,
    pub source_path: String,
    pub line_number: u64,
    pub fact_kind: String,
    pub subject: String,
    pub value: String,
    pub assertion_kind: String,
    pub confidence: String,
    pub parser: String,
    pub source_artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfrastructureNetworkConfigRow {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub line_number: u64,
    pub line: String,
}

pub struct InfrastructureNetworkFactRepo<'a> {
    conn: &'a Connection,
}

impl<'a> InfrastructureNetworkFactRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn replace_for_source(
        &self,
        case_id: &str,
        data_source_id: &str,
        facts: &[InfrastructureNetworkFactRecord],
    ) -> DbResult<()> {
        let transaction = self.conn.unchecked_transaction()?;
        let source_in_case: bool = transaction.query_row(
            "SELECT COUNT(*) = 1 FROM data_sources WHERE id = ?1 AND case_id = ?2",
            params![data_source_id, case_id],
            |row| row.get(0),
        )?;
        if !source_in_case {
            return Err(DbError::System(
                "network fact source is outside the case".to_string(),
            ));
        }
        for fact in facts {
            let host_in_case: bool = transaction.query_row(
                "SELECT COUNT(*) = 1 FROM environment_objects
                 WHERE id = ?1 AND case_id = ?2 AND object_kind = 'physical_host'",
                params![fact.environment_object_id, case_id],
                |row| row.get(0),
            )?;
            if fact.case_id != case_id || fact.data_source_id != data_source_id || !host_in_case {
                return Err(DbError::System(
                    "network fact is outside the source or host scope".to_string(),
                ));
            }
        }
        transaction.execute(
            "DELETE FROM infrastructure_network_facts WHERE case_id = ?1 AND data_source_id = ?2",
            params![case_id, data_source_id],
        )?;
        let mut seen = HashSet::with_capacity(facts.len());
        for fact in facts {
            if !seen.insert(fact_key(fact)) {
                continue;
            }
            transaction.execute(
                "INSERT INTO infrastructure_network_facts (
                    id, case_id, data_source_id, environment_object_id, file_id,
                    source_path, line_number, fact_kind, subject, value, assertion_kind,
                    confidence, parser, source_artifact_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    fact.id,
                    fact.case_id,
                    fact.data_source_id,
                    fact.environment_object_id,
                    fact.file_id,
                    fact.source_path,
                    fact.line_number,
                    fact.fact_kind,
                    fact.subject,
                    fact.value,
                    fact.assertion_kind,
                    fact.confidence,
                    fact.parser,
                    fact.source_artifact_id,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_for_case(&self, case_id: &str) -> DbResult<Vec<InfrastructureNetworkFactRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT id, case_id, data_source_id, environment_object_id, file_id,
                    source_path, line_number, fact_kind, subject, value, assertion_kind,
                    confidence, parser, source_artifact_id
             FROM infrastructure_network_facts WHERE case_id = ?1
             ORDER BY environment_object_id, source_path, line_number, fact_kind, id",
        )?;
        let rows = statement.query_map([case_id], |row| {
            Ok(InfrastructureNetworkFactRecord {
                id: row.get(0)?,
                case_id: row.get(1)?,
                data_source_id: row.get(2)?,
                environment_object_id: row.get(3)?,
                file_id: row.get(4)?,
                source_path: row.get(5)?,
                line_number: row.get(6)?,
                fact_kind: row.get(7)?,
                subject: row.get(8)?,
                value: row.get(9)?,
                assertion_kind: row.get(10)?,
                confidence: row.get(11)?,
                parser: row.get(12)?,
                source_artifact_id: row.get(13)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_host_sources(&self, case_id: &str) -> DbResult<Vec<(String, String)>> {
        let mut statement = self.conn.prepare(
            "SELECT id, json_extract(provenance_json, '$.dataSourceId')
             FROM environment_objects
             WHERE case_id = ?1 AND object_kind = 'physical_host'
               AND json_valid(provenance_json)
               AND json_type(provenance_json, '$.dataSourceId') = 'text'
             ORDER BY id",
        )?;
        let rows = statement.query_map([case_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_source_config_rows(
        source_conn: &Connection,
        limit: usize,
    ) -> DbResult<Vec<InfrastructureNetworkConfigRow>> {
        if crate::detail::source_id(source_conn)?.is_none() {
            return Err(persistence_sqlite::DbError::System(
                "source identity is missing".into(),
            ));
        }
        Self::list_native_source_config_rows(source_conn, limit)
    }

    fn list_native_source_config_rows(
        source_conn: &Connection,
        limit: usize,
    ) -> DbResult<Vec<InfrastructureNetworkConfigRow>> {
        let source_id = crate::detail::source_id(source_conn)?
            .ok_or_else(|| DbError::System("source identity is missing".into()))?;
        let store = crate::detail::require_source_store(source_conn)?;
        let repository =
            persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
        let snapshot = repository.read_snapshot();
        let mut cursor = None;
        let mut rows = Vec::with_capacity(limit.min(512));
        loop {
            persistence_sqlite::read_operation::ensure_active()?;
            let page = repository
                .list_by_type(
                    &snapshot,
                    &source_id,
                    "LinuxSystemConfig",
                    cursor.as_deref(),
                    persistence_fjall::ScanDirection::Forward,
                    512,
                )
                .map_err(|error| DbError::System(error.to_string()))?;
            for record in page.records {
                let artifact = super::artifact_repo::decode_fjall_artifact(record)?;
                let Some(file_id) = artifact.source_object_id.map(|id| id.0) else {
                    continue;
                };
                if artifact
                    .attrs
                    .get("overlayContext")
                    .is_some_and(|value| !value.is_null())
                {
                    continue;
                }
                let source_path = artifact
                    .attrs
                    .get("sourcePath")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if !network_config_path(source_path) {
                    continue;
                }
                rows.push(InfrastructureNetworkConfigRow {
                    artifact_id: artifact.id.0,
                    file_id,
                    source_path: source_path.to_owned(),
                    line_number: artifact
                        .attrs
                        .get("lineNumber")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_default(),
                    line: artifact
                        .attrs
                        .get("line")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                });
                if rows.len() == limit {
                    return Ok(rows);
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(rows)
    }
}

fn network_config_path(path: &str) -> bool {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    [
        "/etc/hosts",
        "/etc/resolv.conf",
        "/etc/network/interfaces",
        "/etc/pve/corosync.conf",
        "/etc/corosync/corosync.conf",
        "/etc/pve/.version",
        "/etc/debian_version",
    ]
    .iter()
    .any(|suffix| path == *suffix || path.ends_with(suffix))
}

fn fact_key(fact: &InfrastructureNetworkFactRecord) -> (&str, &str, &str, u64, &str, &str, &str) {
    (
        &fact.case_id,
        &fact.data_source_id,
        &fact.file_id,
        fact.line_number,
        &fact.fact_kind,
        &fact.subject,
        &fact.value,
    )
}
