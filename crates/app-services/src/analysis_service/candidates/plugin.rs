//! Plugin evidence candidate discovery (design doc §2.1).
//!
//! Loaded plugins declare `path_patterns_json` patterns; a file entry becomes
//! a plugin candidate when at least one loaded plugin's patterns match it
//! (`*.pf` suffix or exact file name, case-insensitive — the same semantics
//! as `ArtifactExtractor::supports_path`). One candidate per file even when
//! several plugins match; every matching plugin runs on it at extraction
//! time.

use super::common::EvidenceCompanion;
use super::common::{candidate_content_identity, normalize_evidence_path};
use super::EvidenceCandidate;
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::capability::PLUGIN_CAPABILITY_KEY;
use crate::analysis_service::error::AnalysisServiceError;
use artifacts_core::ArtifactExtractor;
use domain::FileEntryId;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

struct PluginFileRecord {
    file_id: String,
    data_source_id: String,
    partition_index: Option<usize>,
    path: String,
    size: u64,
    encrypted: bool,
    content_identity: String,
    modified_at: Option<chrono::DateTime<chrono::Utc>>,
}

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
    let records = load_plugin_files(conn, cancel_token)?;
    let by_path = records
        .iter()
        .enumerate()
        .map(|(index, record)| (normalize_evidence_path(&record.path).to_lowercase(), index))
        .collect::<HashMap<_, _>>();
    let mut candidates = Vec::new();
    for record in &records {
        ensure_not_cancelled(cancel_token)?;
        let normalized = normalize_evidence_path(&record.path);
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
        let companions = companion_records(record, &records, &by_path);
        let content_identity = companions.iter().fold(
            format!("plugin-candidate-v2:{}", record.content_identity),
            |mut identity, companion| {
                identity.push(':');
                identity.push_str(&companion.content_identity);
                identity
            },
        );
        candidates.push(EvidenceCandidate {
            file_id: FileEntryId(record.file_id.clone()),
            data_source_id: record.data_source_id.clone(),
            partition_index: record.partition_index,
            path: record.path.clone(),
            size: record.size,
            encrypted: record.encrypted,
            content_identity,
            companions,
            evidence_kind: "plugin".to_string(),
            parser,
            category: PLUGIN_CAPABILITY_KEY.to_string(),
            modified_at: record.modified_at,
        });
    }
    ensure_not_cancelled(cancel_token)?;
    Ok(candidates)
}

fn load_plugin_files(
    conn: &Connection,
    cancel_token: &AtomicBool,
) -> Result<Vec<PluginFileRecord>, AnalysisServiceError> {
    let entries = super::common::collect_file_entries(conn)?;
    let mut records = Vec::new();
    let repository = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
    for entry in entries {
        ensure_not_cancelled(cancel_token)?;
        if !entry.is_file() {
            continue;
        }
        let partition_index = repository.find_partition_index_by_id(&entry.id)?;
        let encryption_status = repository
            .find_encryption_status(&entry.id)?
            .ok_or_else(|| AnalysisServiceError::Other("plugin candidate disappeared".into()))?;
        let file_id = entry.id.0;
        let data_source_id = entry.data_source_id.0;
        let path = entry.path;
        let size = entry.size.unwrap_or_default();
        records.push(PluginFileRecord {
            content_identity: candidate_content_identity(
                &file_id,
                &data_source_id,
                partition_index,
                &path,
                size,
                encryption_status,
                [
                    entry.created_at.map(|value| value.to_rfc3339()),
                    entry.modified_at.map(|value| value.to_rfc3339()),
                    entry.accessed_at.map(|value| value.to_rfc3339()),
                    entry.changed_at.map(|value| value.to_rfc3339()),
                    entry.hash_sha256,
                ],
            ),
            file_id,
            data_source_id,
            partition_index,
            path,
            size,
            encrypted: encryption_status.blocks_content(),
            modified_at: entry.modified_at,
        });
    }
    Ok(records)
}

fn companion_records(
    primary: &PluginFileRecord,
    records: &[PluginFileRecord],
    by_path: &HashMap<String, usize>,
) -> Vec<EvidenceCompanion> {
    let normalized = normalize_evidence_path(&primary.path);
    if !normalized.to_lowercase().ends_with(".db") {
        return Vec::new();
    }
    let wal_path = format!("{normalized}-wal").to_lowercase();
    let Some(companion) = by_path
        .get(&wal_path)
        .and_then(|index| records.get(*index))
        .filter(|record| record.data_source_id == primary.data_source_id)
    else {
        return Vec::new();
    };
    vec![EvidenceCompanion {
        file_id: FileEntryId(companion.file_id.clone()),
        path: companion.path.clone(),
        size: companion.size,
        encrypted: companion.encrypted,
        content_identity: companion.content_identity.clone(),
    }]
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/candidates/plugin.rs"]
mod tests;
