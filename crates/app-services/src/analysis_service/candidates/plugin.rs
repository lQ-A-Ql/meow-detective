//! Plugin evidence candidate discovery (design doc §2.1).
//!
//! Loaded plugins declare `path_patterns_json` patterns; a file entry becomes
//! a plugin candidate when at least one loaded plugin's patterns match it
//! (`*.pf` suffix or exact file name, case-insensitive — the same semantics
//! as `ArtifactExtractor::supports_path`). One candidate per file even when
//! several plugins match; every matching plugin runs on it at extraction
//! time.

use super::common::{
    candidate_content_identity, file_entries_has_partition_index, normalize_evidence_path,
    parse_partition_index, parse_timestamp,
};
use super::EvidenceCandidate;
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::capability::PLUGIN_CAPABILITY_KEY;
use crate::analysis_service::error::AnalysisServiceError;
use artifacts_core::ArtifactExtractor;
use domain::FileEntryId;
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

/// Discover file entries matched by at least one loaded plugin's declared
/// path patterns. Returns an empty vector when no plugins are loaded, which
/// keeps the `plugins.enabled=false` / no-plugin path zero-overhead.
pub fn discover_plugin_candidates(
    conn: &Connection,
    plugins: &[&dyn ArtifactExtractor],
    cancel_token: &AtomicBool,
) -> Result<Vec<EvidenceCandidate>, AnalysisServiceError> {
    if plugins.is_empty() {
        return Ok(Vec::new());
    }
    let partition_column = if file_entries_has_partition_index(conn)? {
        "partition_index"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT id, data_source_id, path, COALESCE(size, 0), {partition_column},
                created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted
         FROM file_entries
         WHERE entry_type = 'file' COLLATE NOCASE"
    );
    let mut statement = conn.prepare(&sql)?;
    let mut rows = statement.query([])?;

    let mut candidates = Vec::new();
    while let Some(row) = rows.next()? {
        ensure_not_cancelled(cancel_token)?;
        let file_id: String = row.get(0)?;
        let path: String = row.get(2)?;
        let normalized = normalize_evidence_path(&path);
        let mut matching = plugins
            .iter()
            .filter(|plugin| plugin.supports_path(&normalized))
            .peekable();
        if matching.peek().is_none() {
            continue;
        }
        let parser = matching
            .map(|plugin| plugin.id().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let data_source_id: String = row.get(1)?;
        let partition_index = parse_partition_index(row, &file_id)?;
        let encryption_status =
            persistence_sqlite::repositories::file_repo::file_encryption_status_from_row(row, 10)?;
        let size: u64 = row.get(3)?;
        let content_identity = candidate_content_identity(
            &file_id,
            &data_source_id,
            partition_index,
            &path,
            size,
            encryption_status,
            [
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
            ],
        );
        candidates.push(EvidenceCandidate {
            file_id: FileEntryId(file_id),
            data_source_id,
            partition_index,
            path,
            size,
            encrypted: encryption_status.blocks_content(),
            content_identity,
            evidence_kind: "plugin".to_string(),
            parser,
            category: PLUGIN_CAPABILITY_KEY.to_string(),
            modified_at: parse_timestamp(row.get::<_, Option<String>>(6)?),
        });
    }
    ensure_not_cancelled(cancel_token)?;
    Ok(candidates)
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/candidates/plugin.rs"]
mod tests;
