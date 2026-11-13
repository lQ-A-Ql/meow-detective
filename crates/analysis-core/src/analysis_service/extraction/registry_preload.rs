use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::MAX_REGISTRY_ANALYSIS_BYTES;
use artifacts_windows::registry::{RegistryHiveSource, RegistrySnapshot};
use domain::FileEntryId;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::atomic::AtomicBool;

const READ_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Default)]
pub(super) struct RegistryPreloadContext {
    snapshots: HashMap<(String, String), RegistrySnapshot<'static>>,
    failures: HashMap<(String, String), String>,
    boot_keys: HashMap<(String, Option<usize>), Option<[u8; 16]>>,
    boot_key_read_failures: HashMap<(String, Option<usize>), String>,
    pub(super) dependency_warnings: Vec<String>,
}

impl RegistryPreloadContext {
    pub(super) fn prepared_records(&self) -> u64 {
        self.snapshots.len() as u64
    }

    pub(super) fn snapshot(
        &self,
        candidate: &EvidenceCandidate,
    ) -> Option<&RegistrySnapshot<'static>> {
        let key = registry_key(candidate);
        self.snapshots.get(&key)
    }

    pub(super) fn failure_reason(&self, candidate: &EvidenceCandidate) -> Option<&str> {
        self.failures
            .get(&registry_key(candidate))
            .map(String::as_str)
    }

    pub(super) fn boot_key(&self, candidate: &EvidenceCandidate) -> Option<[u8; 16]> {
        self.boot_keys
            .get(&(candidate.data_source_id.clone(), candidate.partition_index))
            .copied()
            .flatten()
    }

    pub(super) fn dependency_read_failure(&self, candidate: &EvidenceCandidate) -> Option<&str> {
        let path = normalize_evidence_path(&candidate.path);
        if candidate.category != "Registry"
            || !(path.ends_with("/sam") || path.ends_with("/security"))
        {
            return None;
        }
        self.boot_key_read_failures
            .get(&(candidate.data_source_id.clone(), candidate.partition_index))
            .map(String::as_str)
    }
}

pub(super) fn preload_registry_context<E: std::fmt::Display>(
    candidates: &[EvidenceCandidate],
    cancel_token: &AtomicBool,
    mut file_reader: impl FnMut(&FileEntryId, usize) -> Result<Box<dyn Read>, E>,
    already_extracted: impl Fn(&EvidenceCandidate) -> Result<bool, AnalysisServiceError>,
) -> Result<RegistryPreloadContext, AnalysisServiceError> {
    ensure_not_cancelled(cancel_token)?;
    let mut snapshots = HashMap::new();
    let mut failures = HashMap::new();
    let mut boot_keys = HashMap::new();
    let mut boot_key_read_failures = HashMap::new();
    let mut warnings = Vec::new();
    let mut pending = HashSet::new();
    let mut boot_key_scopes = HashSet::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.category == "Registry")
    {
        ensure_not_cancelled(cancel_token)?;
        if !already_extracted(candidate)? {
            pending.insert(registry_key(candidate));
            let path = normalize_evidence_path(&candidate.path);
            if path.ends_with("/windows/system32/config/sam")
                || path.ends_with("/windows/system32/config/security")
            {
                boot_key_scopes
                    .insert((candidate.data_source_id.clone(), candidate.partition_index));
            }
        }
    }

    for candidate in candidates {
        ensure_not_cancelled(cancel_token)?;
        if candidate.category != "Registry" {
            continue;
        }
        let is_system =
            normalize_evidence_path(&candidate.path).ends_with("/windows/system32/config/system");
        let needs_boot_key = is_system
            && boot_key_scopes
                .contains(&(candidate.data_source_id.clone(), candidate.partition_index));
        let key = registry_key(candidate);
        let is_pending = pending.contains(&key);
        if !is_pending && !needs_boot_key {
            continue;
        }
        let snapshot = match load_snapshot(candidate, cancel_token, &mut file_reader) {
            Ok(snapshot) => snapshot,
            Err(RegistryPreparationError::Control(error)) => return Err(error),
            Err(RegistryPreparationError::Read(reason)) => {
                if is_system && needs_boot_key {
                    boot_key_read_failures.insert(
                        (candidate.data_source_id.clone(), candidate.partition_index),
                        reason.clone(),
                    );
                }
                if is_pending {
                    failures.insert(key, reason);
                } else {
                    warnings.push(reason);
                }
                continue;
            }
            Err(RegistryPreparationError::Input(reason)) => {
                if is_pending {
                    failures.insert(key, reason);
                } else {
                    warnings.push(reason);
                }
                continue;
            }
        };
        if is_system {
            boot_keys.insert(
                (candidate.data_source_id.clone(), candidate.partition_index),
                artifacts_windows::extract_boot_key(&snapshot),
            );
        }
        snapshots.insert(registry_key(candidate), snapshot);
    }

    ensure_not_cancelled(cancel_token)?;
    Ok(RegistryPreloadContext {
        snapshots,
        failures,
        boot_keys,
        boot_key_read_failures,
        dependency_warnings: warnings,
    })
}

#[derive(Debug, thiserror::Error)]
enum RegistryPreparationError {
    #[error(transparent)]
    Control(#[from] AnalysisServiceError),
    #[error("{0}")]
    Input(String),
    #[error("{0}")]
    Read(String),
}

fn load_snapshot<E: std::fmt::Display>(
    candidate: &EvidenceCandidate,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&FileEntryId, usize) -> Result<Box<dyn Read>, E>,
) -> Result<RegistrySnapshot<'static>, RegistryPreparationError> {
    let bytes = read_file_entry_bytes(
        file_reader,
        &candidate.file_id,
        &candidate.path,
        None,
        cancel_token,
    )?;
    if bytes.len() as u64 != candidate.size {
        return Err(RegistryPreparationError::Read(read_warning(
            &candidate.path,
            None,
            &"registry hive was truncated",
        )));
    }
    let mut logs = Vec::with_capacity(2);
    for label in ["LOG1", "LOG2"] {
        let path = format!(
            "{}.{}",
            normalize_evidence_path(&candidate.path),
            label.to_ascii_lowercase()
        );
        let companion = candidate
            .companions
            .iter()
            .find(|item| normalize_evidence_path(&item.path) == path);
        let Some(companion) = companion else {
            logs.push(None);
            continue;
        };
        let bytes = read_file_entry_bytes(
            file_reader,
            &companion.file_id,
            &candidate.path,
            Some(label),
            cancel_token,
        )?;
        if bytes.len() as u64 != companion.size {
            return Err(RegistryPreparationError::Read(read_warning(
                &candidate.path,
                Some(label),
                &"registry log was truncated",
            )));
        }
        logs.push(Some(bytes));
    }
    ensure_not_cancelled(cancel_token)?;
    RegistrySnapshot::from_owned(bytes, logs[0].as_deref(), logs[1].as_deref())
        .and_then(|snapshot| {
            snapshot.registry_view()?;
            Ok(snapshot)
        })
        .map_err(|error| {
            RegistryPreparationError::Input(read_warning(&candidate.path, None, &error))
        })
}

fn read_file_entry_bytes<E: std::fmt::Display>(
    file_reader: &mut impl FnMut(&FileEntryId, usize) -> Result<Box<dyn Read>, E>,
    file_id: &FileEntryId,
    hive_path: &str,
    label: Option<&str>,
    cancel_token: &AtomicBool,
) -> Result<Vec<u8>, RegistryPreparationError> {
    ensure_not_cancelled(cancel_token)?;
    let result = file_reader(file_id, MAX_REGISTRY_ANALYSIS_BYTES);
    ensure_not_cancelled(cancel_token)?;
    let mut reader = result
        .map_err(|error| RegistryPreparationError::Read(read_warning(hive_path, label, &error)))?;
    let mut bytes = Vec::with_capacity(READ_BUFFER_BYTES);
    let mut buffer = [0u8; READ_BUFFER_BYTES];
    let mut limited = reader.by_ref().take(MAX_REGISTRY_ANALYSIS_BYTES as u64 + 1);
    loop {
        ensure_not_cancelled(cancel_token)?;
        let result = limited.read(&mut buffer);
        ensure_not_cancelled(cancel_token)?;
        let read = result.map_err(|error| {
            RegistryPreparationError::Read(read_warning(hive_path, label, &error))
        })?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_REGISTRY_ANALYSIS_BYTES {
            return Err(RegistryPreparationError::Read(read_warning(
                hive_path,
                label,
                &"registry input exceeds the byte limit",
            )));
        }
    }
    ensure_not_cancelled(cancel_token)?;
    Ok(bytes)
}

fn read_warning(hive_path: &str, label: Option<&str>, error: &impl std::fmt::Display) -> String {
    match label {
        Some(label) => format!("{hive_path} {label} read failed: {error}"),
        None => format!("{hive_path} read failed: {error}"),
    }
}

fn registry_key(candidate: &EvidenceCandidate) -> (String, String) {
    (
        candidate.data_source_id.clone(),
        candidate.file_id.0.clone(),
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/registry_preload.rs"]
mod tests;
