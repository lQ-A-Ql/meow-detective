use crate::connection::{DbError, DbResult};
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
        let mut statement = source_conn.prepare(
            "SELECT id, source_object_id, attrs
             FROM artifacts
             WHERE artifact_type = 'LinuxSystemConfig'
               AND source_object_id IS NOT NULL
               AND json_valid(attrs)
               AND json_extract(attrs, '$.overlayContext') IS NULL
               AND (
                 LOWER(REPLACE(json_extract(attrs, '$.sourcePath'), '\', '/')) LIKE '%/etc/hosts'
                 OR LOWER(REPLACE(json_extract(attrs, '$.sourcePath'), '\', '/')) LIKE '%/etc/resolv.conf'
                 OR LOWER(REPLACE(json_extract(attrs, '$.sourcePath'), '\', '/')) LIKE '%/etc/network/interfaces'
                 OR LOWER(REPLACE(json_extract(attrs, '$.sourcePath'), '\', '/')) LIKE '%/etc/pve/corosync.conf'
                 OR LOWER(REPLACE(json_extract(attrs, '$.sourcePath'), '\', '/')) LIKE '%/etc/corosync/corosync.conf'
                 OR LOWER(REPLACE(json_extract(attrs, '$.sourcePath'), '\', '/')) LIKE '%/etc/pve/.version'
                 OR LOWER(REPLACE(json_extract(attrs, '$.sourcePath'), '\', '/')) LIKE '%/etc/debian_version'
               )
             ORDER BY id LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            let attrs: String = row.get(2)?;
            let value: serde_json::Value = serde_json::from_str(&attrs).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(InfrastructureNetworkConfigRow {
                artifact_id: row.get(0)?,
                file_id: row.get(1)?,
                source_path: value["sourcePath"].as_str().unwrap_or_default().to_string(),
                line_number: value["lineNumber"].as_u64().unwrap_or_default(),
                line: value["line"].as_str().unwrap_or_default().to_string(),
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
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
