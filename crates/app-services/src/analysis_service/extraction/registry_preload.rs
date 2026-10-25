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

pub(super) struct RegistryPreloadContext {
    snapshots: HashMap<(String, String), RegistrySnapshot<'static>>,
    boot_keys: HashMap<(String, Option<usize>), Option<[u8; 16]>>,
    pub(super) warnings: Vec<String>,
}

impl RegistryPreloadContext {
    pub(super) fn snapshot(
        &self,
        candidate: &EvidenceCandidate,
    ) -> Option<&RegistrySnapshot<'static>> {
        let key = registry_key(candidate);
        self.snapshots.get(&key)
    }

    pub(super) fn boot_key(&self, candidate: &EvidenceCandidate) -> Option<[u8; 16]> {
        self.boot_keys
            .get(&(candidate.data_source_id.clone(), candidate.partition_index))
            .copied()
            .flatten()
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
    let mut boot_keys = HashMap::new();
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
        if !pending.contains(&registry_key(candidate)) && !needs_boot_key {
            continue;
        }
        let Some(snapshot) =
            load_snapshot(candidate, cancel_token, &mut file_reader, &mut warnings)?
        else {
            continue;
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
        boot_keys,
        warnings,
    })
}

fn load_snapshot<E: std::fmt::Display>(
    candidate: &EvidenceCandidate,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&FileEntryId, usize) -> Result<Box<dyn Read>, E>,
    warnings: &mut Vec<String>,
) -> Result<Option<RegistrySnapshot<'static>>, AnalysisServiceError> {
    let Some(bytes) = read_file_entry_bytes(
        file_reader,
        &candidate.file_id,
        &candidate.path,
        None,
        cancel_token,
        warnings,
    )?
    else {
        return Ok(None);
    };
    if bytes.len() as u64 != candidate.size {
        warnings.push(read_warning(
            &candidate.path,
            None,
            &"registry hive was truncated",
        ));
        return Ok(None);
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
        let Some(bytes) = read_file_entry_bytes(
            file_reader,
            &companion.file_id,
            &candidate.path,
            Some(label),
            cancel_token,
            warnings,
        )?
        else {
            return Ok(None);
        };
        if bytes.len() as u64 != companion.size {
            warnings.push(read_warning(
                &candidate.path,
                Some(label),
                &"registry log was truncated",
            ));
            return Ok(None);
        }
        logs.push(Some(bytes));
    }
    ensure_not_cancelled(cancel_token)?;
    let snapshot = RegistrySnapshot::from_owned(bytes, logs[0].as_deref(), logs[1].as_deref())
        .and_then(|snapshot| {
            snapshot.registry_view()?;
            Ok(snapshot)
        });
    match snapshot {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) => {
            warnings.push(read_warning(&candidate.path, None, &error));
            Ok(None)
        }
    }
}

fn read_file_entry_bytes<E: std::fmt::Display>(
    file_reader: &mut impl FnMut(&FileEntryId, usize) -> Result<Box<dyn Read>, E>,
    file_id: &FileEntryId,
    hive_path: &str,
    label: Option<&str>,
    cancel_token: &AtomicBool,
    warnings: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, AnalysisServiceError> {
    ensure_not_cancelled(cancel_token)?;
    let mut reader = match file_reader(file_id, MAX_REGISTRY_ANALYSIS_BYTES) {
        Ok(reader) => reader,
        Err(err) => {
            warnings.push(read_warning(hive_path, label, &err));
            return Ok(None);
        }
    };
    ensure_not_cancelled(cancel_token)?;
    let mut bytes = Vec::with_capacity(READ_BUFFER_BYTES);
    let mut buffer = [0u8; READ_BUFFER_BYTES];
    let mut limited = reader.by_ref().take(MAX_REGISTRY_ANALYSIS_BYTES as u64 + 1);
    loop {
        ensure_not_cancelled(cancel_token)?;
        let read = match limited.read(&mut buffer) {
            Ok(read) => read,
            Err(err) => {
                warnings.push(read_warning(hive_path, label, &err));
                return Ok(None);
            }
        };
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_REGISTRY_ANALYSIS_BYTES {
            warnings.push(read_warning(
                hive_path,
                label,
                &"registry input exceeds the byte limit",
            ));
            return Ok(None);
        }
    }
    ensure_not_cancelled(cancel_token)?;
    Ok(Some(bytes))
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
