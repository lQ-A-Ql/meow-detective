//! Compile multi-file systemd evidence before single-file checkpoint decisions.
use super::{reader::read_candidate_bytes_with_progress, CandidateSource, ExtractionOutcome};
use crate::analysis_service::{
    cancellation::ensure_not_cancelled, candidates::EvidenceCandidate, error::AnalysisServiceError,
};
use artifacts_linux::{LinuxArtifactError, LinuxServiceUnit, SystemdUnitCatalog, SystemdUnitFile};
use domain::{DataSourceId, FileEntryId};
use persistence_sqlite::repositories::systemd_unit_repo::{SystemdCatalogRow, SystemdUnitRepo};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::atomic::AtomicBool,
};

#[derive(Default)]
pub(super) struct SystemdPreload {
    markers: BTreeMap<String, (u64, bool)>,
    files: HashMap<String, PreparedFile>,
    pub(super) identities: HashMap<String, String>,
}
#[derive(Default)]
struct PreparedFile {
    units: Vec<(LinuxServiceUnit, serde_json::Value)>,
    warnings: Vec<String>,
    errors: Vec<(String, String)>,
}
type Scope = (String, Option<u32>);
impl SystemdPreload {
    pub(super) fn annotate(&self, candidates: &mut [EvidenceCandidate]) {
        for candidate in candidates {
            if let Some(identity) = self.identities.get(&candidate.file_id.0) {
                candidate.content_identity = identity.clone();
            }
        }
    }
    pub(super) fn outcome(&self, candidate: &EvidenceCandidate) -> Option<ExtractionOutcome> {
        let file = self.files.get(&candidate.file_id.0)?;
        let mut outcome = ExtractionOutcome {
            warnings: file.warnings.clone(),
            ..Default::default()
        };
        for (unit, references) in &file.units {
            super::linux::append_resolved_unit(candidate, unit, &mut outcome);
            if let Some(artifact) = outcome.artifacts.last_mut() {
                artifact
                    .attrs
                    .insert("sourceFiles".into(), references.clone());
                artifact
                    .attrs
                    .insert("systemdSchemaVersion".into(), 2.into());
            }
        }
        for (name, message) in &file.errors {
            let mut attrs = crate::analysis_service::artifact_builders::base_attrs(candidate);
            attrs.insert("configKind".into(), "systemdServiceUnit".into());
            attrs.insert("name".into(), name.clone().into());
            attrs.insert("state".into(), "unknown".into());
            attrs.insert("systemdSchemaVersion".into(), 2.into());
            attrs.insert("diagnostic".into(), message.clone().into());
            outcome
                .artifacts
                .push(crate::analysis_service::artifact_builders::make_artifact(
                    "LinuxSystemConfig",
                    format!("systemd unit: {name}"),
                    "systemd unit could not be resolved".into(),
                    candidate,
                    "linux.systemd_unit",
                    attrs,
                ));
        }
        Some(outcome)
    }
    pub(super) fn persist_markers(
        &self,
        conn: &rusqlite::Connection,
    ) -> Result<(), AnalysisServiceError> {
        // Preserve the legacy flat-database extraction API. Source-backed
        // production analyses always have their source_meta summary store.
        if !persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(conn)
            .table_available()?
        {
            return Ok(());
        }
        let tx = conn.unchecked_transaction()?;
        for (source, (count, complete)) in &self.markers {
            persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(&tx).write(&format!("systemd.inventory:{source}"),&serde_json::json!({"schemaVersion":2,"unitCount":count,"complete":complete,"generatedAt":chrono::Utc::now().to_rfc3339()}).to_string())?;
        }
        tx.commit()?;
        Ok(())
    }
}
pub(super) fn prepare(
    conn: &rusqlite::Connection,
    candidates: &[EvidenceCandidate],
    cancel: &AtomicBool,
    read: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<SystemdPreload, AnalysisServiceError> {
    let selected = candidates
        .iter()
        .filter(|candidate| {
            super::linux::is_systemd_unit_path(
                &crate::analysis_service::candidates::normalize_evidence_path(&candidate.path),
            )
        })
        .map(|candidate| candidate.data_source_id.clone())
        .collect::<BTreeSet<_>>();
    let mut groups = BTreeMap::<Scope, BTreeMap<String, SystemdCatalogRow>>::new();
    for source in selected {
        let mut after = String::new();
        loop {
            ensure_not_cancelled(cancel)?;
            let rows = SystemdUnitRepo::new(conn).catalog_page(&source, &after)?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                after = row.file_id.clone();
                let path = unit_path(&row);
                let group = groups
                    .entry((source.clone(), row.partition_index))
                    .or_default();
                if group.len() >= 32768 {
                    return Err(AnalysisServiceError::Unsupported(
                        "systemd catalog exceeds 32768 entries per source partition".into(),
                    ));
                }
                group.insert(path, row);
            }
        }
    }
    let mut preload = SystemdPreload::default();
    for (_, rows) in groups {
        ensure_not_cancelled(cancel)?;
        prepare_scope(&rows, cancel, read, &mut preload)?;
    }
    Ok(preload)
}
fn prepare_scope(
    rows: &BTreeMap<String, SystemdCatalogRow>,
    cancel: &AtomicBool,
    read: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    preload: &mut SystemdPreload,
) -> Result<(), AnalysisServiceError> {
    let mut cache = BTreeMap::<String, Vec<u8>>::new();
    let mut cache_bytes = 0usize;
    {
        let mut read_file = |path: &str, maximum: usize| -> Result<Vec<u8>, LinuxArtifactError> {
            if let Some(bytes) = cache.get(path) {
                return Ok(bytes[..bytes.len().min(maximum)].to_vec());
            }
            let row = rows
                .get(path)
                .ok_or_else(|| parse_error("unit dependency is outside the source catalog"))?;
            if row.encrypted || row.size > 1024 * 1024 {
                return Err(parse_error(
                    "unit dependency is unreadable or exceeds limit",
                ));
            }
            let candidate = reader_candidate(row);
            let bytes = read_candidate_bytes_with_progress(
                &candidate,
                1024 * 1024 + 1,
                cancel,
                read,
                |_| {},
            )
            .map_err(|_| parse_error("unit dependency could not be read"))?;
            if bytes.len() as u64 != row.size {
                return Err(parse_error(
                    "unit dependency length changed or was truncated",
                ));
            }
            cache_bytes = cache_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| parse_error("unit cache size overflow"))?;
            if cache_bytes > 32 * 1024 * 1024 {
                return Err(parse_error("systemd input set exceeds the 32 MiB budget"));
            }
            cache.insert(path.into(), bytes.clone());
            Ok(bytes[..bytes.len().min(maximum)].to_vec())
        };
        for row in rows.values() {
            preload.files.entry(row.file_id.clone()).or_default();
        }
        let catalog = SystemdUnitCatalog::new(rows.iter().map(|(path, row)| {
            SystemdUnitFile {
                path: path.clone(),
                is_symlink: row
                    .unix_mode
                    .is_some_and(|mode| mode & 0o170000 == 0o120000),
            }
        }));
        let catalog =
            catalog.map_err(|error| AnalysisServiceError::Unsupported(error.to_string()))?;
        compile_units(rows, &catalog, &mut read_file, preload, cancel)?;
    }
    let mut fingerprint = Sha256::new();
    fingerprint.update(b"systemd-v2");
    for (path, bytes) in cache {
        fingerprint.update(path.as_bytes());
        fingerprint.update(Sha256::digest(bytes));
    }
    for (path, row) in rows {
        fingerprint.update(path.as_bytes());
        fingerprint.update(row.file_id.as_bytes());
        fingerprint.update(row.size.to_le_bytes());
    }
    let revision = hex::encode(fingerprint.finalize());
    for row in rows.values() {
        preload
            .identities
            .insert(row.file_id.clone(), revision.clone());
    }
    if let Some(source) = rows.values().next().map(|row| row.source_id.clone()) {
        let count = rows
            .values()
            .filter_map(|row| preload.files.get(&row.file_id))
            .map(|file| (file.units.len() + file.errors.len()) as u64)
            .sum::<u64>();
        let complete = rows
            .values()
            .filter_map(|row| preload.files.get(&row.file_id))
            .all(|file| file.warnings.is_empty() && file.errors.is_empty());
        let marker = preload.markers.entry(source).or_insert((0, true));
        marker.0 += count;
        marker.1 &= complete;
    }
    Ok(())
}
fn unit_path(row: &SystemdCatalogRow) -> String {
    crate::analysis_service::candidates::evidence_relative_path(&row.path)
}
fn reader_candidate(row: &SystemdCatalogRow) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(row.file_id.clone()),
        data_source_id: row.source_id.clone(),
        partition_index: row.partition_index.map(|index| index as usize),
        path: row.path.clone(),
        size: row.size,
        encrypted: row.encrypted,
        content_identity: String::new(),
        companions: Vec::new(),
        evidence_kind: "systemd".into(),
        parser: "linux.systemd_unit".into(),
        category: "LinuxArtifacts".into(),
        modified_at: None,
    }
}
fn parse_error(message: &str) -> LinuxArtifactError {
    LinuxArtifactError::ParseError {
        parser: "systemd.unit",
        message: message.into(),
    }
}

fn compile_units(
    rows: &BTreeMap<String, SystemdCatalogRow>,
    catalog: &SystemdUnitCatalog,
    read_file: &mut impl FnMut(&str, usize) -> Result<Vec<u8>, LinuxArtifactError>,
    preload: &mut SystemdPreload,
    cancel: &AtomicBool,
) -> Result<(), AnalysisServiceError> {
    let links = catalog.prepare_links(&mut *read_file);
    let mut seen = BTreeSet::new();
    let mut output_bytes = 0usize;
    for name in catalog.unit_names() {
        ensure_not_cancelled(cancel)?;
        match catalog.resolve_with_links(&name, &links, &mut *read_file) {
            Ok(unit) => {
                if !seen.insert(unit.name.clone()) {
                    continue;
                }
                let Some(owner) = rows.get(&unit.unit_file) else {
                    return Err(AnalysisServiceError::Extraction(
                        "systemd owner is outside source catalog".into(),
                    ));
                };
                let references=serde_json::Value::Array(unit.source_paths.iter().filter_map(|path|rows.get(path)).map(|row|serde_json::json!({"fileId":crate::source_db::encode_source_scoped_id(&DataSourceId(row.source_id.clone()),&row.file_id),"path":row.path})).collect());
                output_bytes = output_bytes.saturating_add(
                    unit.exec_start
                        .iter()
                        .chain(&unit.exec_stop)
                        .map(String::len)
                        .sum::<usize>(),
                );
                if output_bytes > 64 * 1024 * 1024 {
                    preload
                        .files
                        .entry(owner.file_id.clone())
                        .or_default()
                        .errors
                        .push((unit.name.clone(), "compiled metadata exceeds limit".into()));
                    preload
                        .files
                        .entry(owner.file_id.clone())
                        .or_default()
                        .warnings
                        .push("systemd compiled metadata exceeds the 64 MiB budget".into());
                    continue;
                }
                preload
                    .files
                    .entry(owner.file_id.clone())
                    .or_default()
                    .units
                    .push((unit, references));
            }
            Err(error) => {
                if let Some(owner) = rows.values().find(|row| row.name == name) {
                    preload
                        .files
                        .entry(owner.file_id.clone())
                        .or_default()
                        .errors
                        .push((name.clone(), error.to_string()));
                    preload
                        .files
                        .entry(owner.file_id.clone())
                        .or_default()
                        .warnings
                        .push(format!("{}: {error}", owner.path));
                }
            }
        }
    }
    Ok(())
}
