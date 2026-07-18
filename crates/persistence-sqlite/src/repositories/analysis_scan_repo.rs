use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};

const CLEAN_SCAN_KEY_PREFIX: &str = "analysis_candidate_scan:clean:";
const DIAGNOSTIC_SCAN_KEY_PREFIX: &str = "analysis_candidate_scan:diagnostic:";
const COMPLETE_SCAN_KEY_PREFIX: &str = "analysis_candidate_scan:complete:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanAnalysisCandidateScan {
    pub source_object_id: String,
    pub capability_key: String,
    pub extractor_version: String,
    pub source_size: u64,
    pub content_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticAnalysisCandidateScan {
    pub source_object_id: String,
    pub capability_key: String,
    pub extractor_version: String,
    pub source_size: u64,
    pub content_identity: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAnalysisCandidateScan {
    pub source_object_id: String,
    pub capability_key: String,
    pub extractor_version: String,
    pub source_size: u64,
    pub content_identity: String,
    pub artifact_count: u64,
    pub timeline_event_count: u64,
    pub output_digest: String,
    pub warnings: Vec<String>,
}

pub struct AnalysisScanRepo<'a> {
    conn: &'a Connection,
}

impl<'a> AnalysisScanRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn storage_available(&self) -> DbResult<bool> {
        self.source_meta_available()
    }

    pub fn is_clean(
        &self,
        source_object_id: &str,
        capability_key: &str,
        extractor_version: &str,
        source_size: u64,
        content_identity: &str,
    ) -> DbResult<bool> {
        if !self.source_meta_available()? {
            return Ok(false);
        }
        let key = clean_scan_key(source_object_id, capability_key, extractor_version);
        let value = self
            .conn
            .query_row(
                "SELECT value FROM source_meta WHERE key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(value) = value else {
            return Ok(false);
        };
        let stored = decode_scan(&value)?;
        Ok(stored.source_object_id == source_object_id
            && stored.capability_key == capability_key
            && stored.extractor_version == extractor_version
            && stored.source_size == source_size
            && stored.content_identity == content_identity)
    }

    pub fn insert_clean_batch(&self, scans: &[CleanAnalysisCandidateScan]) -> DbResult<()> {
        self.insert_checkpoint_batch(scans, &[])
    }

    pub fn insert_diagnostic_batch(
        &self,
        scans: &[DiagnosticAnalysisCandidateScan],
    ) -> DbResult<()> {
        self.insert_checkpoint_batch(&[], scans)
    }

    pub fn insert_checkpoint_batch(
        &self,
        clean_scans: &[CleanAnalysisCandidateScan],
        diagnostic_scans: &[DiagnosticAnalysisCandidateScan],
    ) -> DbResult<()> {
        self.insert_all_checkpoint_batch(clean_scans, diagnostic_scans, &[])
    }

    pub fn insert_all_checkpoint_batch(
        &self,
        clean_scans: &[CleanAnalysisCandidateScan],
        diagnostic_scans: &[DiagnosticAnalysisCandidateScan],
        complete_scans: &[CompleteAnalysisCandidateScan],
    ) -> DbResult<()> {
        if (clean_scans.is_empty() && diagnostic_scans.is_empty() && complete_scans.is_empty())
            || !self.source_meta_available()?
        {
            return Ok(());
        }
        let transaction = self.conn.unchecked_transaction()?;
        AnalysisScanRepo::new(&transaction).insert_all_checkpoint_batch_in_transaction(
            clean_scans,
            diagnostic_scans,
            complete_scans,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_all_checkpoint_batch_in_transaction(
        &self,
        clean_scans: &[CleanAnalysisCandidateScan],
        diagnostic_scans: &[DiagnosticAnalysisCandidateScan],
        complete_scans: &[CompleteAnalysisCandidateScan],
    ) -> DbResult<()> {
        if !self.source_meta_available()? {
            return Ok(());
        }
        let mut statement = self.conn.prepare_cached(
            "INSERT INTO source_meta (key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )?;
        for scan in clean_scans {
            statement.execute(params![
                clean_scan_key(
                    &scan.source_object_id,
                    &scan.capability_key,
                    &scan.extractor_version
                ),
                encode_scan(scan),
            ])?;
        }
        for scan in diagnostic_scans {
            statement.execute(params![
                diagnostic_scan_key(
                    &scan.source_object_id,
                    &scan.capability_key,
                    &scan.extractor_version
                ),
                encode_diagnostic_scan(scan),
            ])?;
        }
        for scan in complete_scans {
            statement.execute(params![
                complete_scan_key(
                    &scan.source_object_id,
                    &scan.capability_key,
                    &scan.extractor_version
                ),
                encode_complete_scan(scan),
            ])?;
        }
        Ok(())
    }

    pub fn list_clean_for_version(
        &self,
        extractor_version: &str,
    ) -> DbResult<Vec<CleanAnalysisCandidateScan>> {
        if !self.source_meta_available()? {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT value
             FROM source_meta
             WHERE key LIKE ?1
             ORDER BY key ASC",
        )?;
        let prefix = version_key_prefix(CLEAN_SCAN_KEY_PREFIX, extractor_version);
        let rows = statement.query_map([format!("{prefix}%")], |row| row.get::<_, String>(0))?;
        rows.map(|row| decode_scan(&row?)).collect()
    }

    pub fn list_diagnostics_for_version(
        &self,
        extractor_version: &str,
    ) -> DbResult<Vec<DiagnosticAnalysisCandidateScan>> {
        if !self.source_meta_available()? {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT value
             FROM source_meta
             WHERE key LIKE ?1
             ORDER BY key ASC",
        )?;
        let prefix = version_key_prefix(DIAGNOSTIC_SCAN_KEY_PREFIX, extractor_version);
        let rows = statement.query_map([format!("{prefix}%")], |row| row.get::<_, String>(0))?;
        rows.map(|row| decode_diagnostic_scan(&row?)).collect()
    }

    pub fn list_complete_for_version(
        &self,
        extractor_version: &str,
    ) -> DbResult<Vec<CompleteAnalysisCandidateScan>> {
        if !self.source_meta_available()? {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT value
             FROM source_meta
             WHERE key LIKE ?1
             ORDER BY key ASC",
        )?;
        let prefix = version_key_prefix(COMPLETE_SCAN_KEY_PREFIX, extractor_version);
        let rows = statement.query_map([format!("{prefix}%")], |row| row.get::<_, String>(0))?;
        rows.map(|row| decode_complete_scan(&row?)).collect()
    }

    fn source_meta_available(&self) -> DbResult<bool> {
        self.conn
            .query_row(
                "SELECT COUNT(*) > 0
                 FROM sqlite_master
                 WHERE type = 'table' AND name = 'source_meta'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }
}

fn version_key_prefix(key_prefix: &str, extractor_version: &str) -> String {
    format!(
        "{key_prefix}{}:{extractor_version}:",
        extractor_version.len()
    )
}

fn clean_scan_key(source_object_id: &str, capability_key: &str, extractor_version: &str) -> String {
    format!(
        "{}{}:{capability_key}:{}:{source_object_id}",
        version_key_prefix(CLEAN_SCAN_KEY_PREFIX, extractor_version),
        capability_key.len(),
        source_object_id.len()
    )
}

fn diagnostic_scan_key(
    source_object_id: &str,
    capability_key: &str,
    extractor_version: &str,
) -> String {
    format!(
        "{}{}:{capability_key}:{}:{source_object_id}",
        version_key_prefix(DIAGNOSTIC_SCAN_KEY_PREFIX, extractor_version),
        capability_key.len(),
        source_object_id.len()
    )
}

fn complete_scan_key(
    source_object_id: &str,
    capability_key: &str,
    extractor_version: &str,
) -> String {
    format!(
        "{}{}:{capability_key}:{}:{source_object_id}",
        version_key_prefix(COMPLETE_SCAN_KEY_PREFIX, extractor_version),
        capability_key.len(),
        source_object_id.len()
    )
}

fn decode_scan(value: &str) -> DbResult<CleanAnalysisCandidateScan> {
    let value: serde_json::Value = serde_json::from_str(value).map_err(|error| {
        DbError::System(format!(
            "decode analysis candidate scan checkpoint: {error}"
        ))
    })?;
    Ok(CleanAnalysisCandidateScan {
        source_object_id: json_string(&value, "sourceObjectId")?,
        capability_key: json_string(&value, "capabilityKey")?,
        extractor_version: json_string(&value, "extractorVersion")?,
        source_size: json_source_size(&value)?,
        content_identity: optional_json_string(&value, "contentIdentity"),
    })
}

fn encode_scan(scan: &CleanAnalysisCandidateScan) -> String {
    serde_json::json!({
        "sourceObjectId": scan.source_object_id,
        "capabilityKey": scan.capability_key,
        "extractorVersion": scan.extractor_version,
        "sourceSize": scan.source_size,
        "contentIdentity": scan.content_identity,
    })
    .to_string()
}

fn decode_diagnostic_scan(value: &str) -> DbResult<DiagnosticAnalysisCandidateScan> {
    let value: serde_json::Value = serde_json::from_str(value).map_err(|error| {
        DbError::System(format!(
            "decode analysis candidate diagnostic checkpoint: {error}"
        ))
    })?;
    let warnings = value
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            DbError::System(
                "analysis candidate diagnostic checkpoint has invalid warnings".to_string(),
            )
        })?
        .iter()
        .map(|warning| {
            warning.as_str().map(str::to_string).ok_or_else(|| {
                DbError::System(
                    "analysis candidate diagnostic checkpoint has a non-string warning".to_string(),
                )
            })
        })
        .collect::<DbResult<Vec<_>>>()?;
    if warnings.is_empty() {
        return Err(DbError::System(
            "analysis candidate diagnostic checkpoint has no warnings".to_string(),
        ));
    }
    Ok(DiagnosticAnalysisCandidateScan {
        source_object_id: json_string(&value, "sourceObjectId")?,
        capability_key: json_string(&value, "capabilityKey")?,
        extractor_version: json_string(&value, "extractorVersion")?,
        source_size: json_source_size(&value)?,
        content_identity: optional_json_string(&value, "contentIdentity"),
        warnings,
    })
}

fn encode_diagnostic_scan(scan: &DiagnosticAnalysisCandidateScan) -> String {
    serde_json::json!({
        "sourceObjectId": scan.source_object_id,
        "capabilityKey": scan.capability_key,
        "extractorVersion": scan.extractor_version,
        "sourceSize": scan.source_size,
        "contentIdentity": scan.content_identity,
        "warnings": scan.warnings,
    })
    .to_string()
}

fn decode_complete_scan(value: &str) -> DbResult<CompleteAnalysisCandidateScan> {
    let value: serde_json::Value = serde_json::from_str(value).map_err(|error| {
        DbError::System(format!(
            "decode complete analysis candidate checkpoint: {error}"
        ))
    })?;
    Ok(CompleteAnalysisCandidateScan {
        source_object_id: json_string(&value, "sourceObjectId")?,
        capability_key: json_string(&value, "capabilityKey")?,
        extractor_version: json_string(&value, "extractorVersion")?,
        source_size: json_source_size(&value)?,
        content_identity: optional_json_string(&value, "contentIdentity"),
        artifact_count: json_u64(&value, "artifactCount")?,
        timeline_event_count: json_u64(&value, "timelineEventCount")?,
        output_digest: json_string(&value, "outputDigest")?,
        warnings: optional_warnings(&value)?,
    })
}

fn encode_complete_scan(scan: &CompleteAnalysisCandidateScan) -> String {
    serde_json::json!({
        "sourceObjectId": scan.source_object_id,
        "capabilityKey": scan.capability_key,
        "extractorVersion": scan.extractor_version,
        "sourceSize": scan.source_size,
        "contentIdentity": scan.content_identity,
        "artifactCount": scan.artifact_count,
        "timelineEventCount": scan.timeline_event_count,
        "outputDigest": scan.output_digest,
        "warnings": scan.warnings,
    })
    .to_string()
}

fn json_string(value: &serde_json::Value, field: &str) -> DbResult<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DbError::System(format!(
                "analysis candidate scan checkpoint has invalid {field}"
            ))
        })
}

fn json_source_size(value: &serde_json::Value) -> DbResult<u64> {
    json_u64(value, "sourceSize")
}

fn optional_json_string(value: &serde_json::Value, field: &str) -> String {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional_warnings(value: &serde_json::Value) -> DbResult<Vec<String>> {
    let Some(warnings) = value.get("warnings") else {
        return Ok(Vec::new());
    };
    let warnings = warnings.as_array().ok_or_else(|| {
        DbError::System("complete analysis candidate checkpoint has invalid warnings".to_string())
    })?;
    warnings
        .iter()
        .map(|warning| {
            warning.as_str().map(str::to_string).ok_or_else(|| {
                DbError::System(
                    "complete analysis candidate checkpoint has a non-string warning".to_string(),
                )
            })
        })
        .collect()
}

fn json_u64(value: &serde_json::Value, field: &str) -> DbResult<u64> {
    value
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            DbError::System(format!(
                "analysis candidate scan checkpoint has invalid {field}"
            ))
        })
}
