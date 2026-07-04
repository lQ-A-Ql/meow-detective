use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use domain::FileEntryId;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use std::io::Read;

type TxlogBytes = (Option<Vec<u8>>, Option<Vec<u8>>);

pub(super) struct RegistryPreloadContext {
    registry_bytes: HashMap<(String, String), Vec<u8>>,
    txlog_bytes: HashMap<(String, String), TxlogBytes>,
    boot_keys: HashMap<String, Option<[u8; 16]>>,
    pub(super) warnings: Vec<String>,
}

impl RegistryPreloadContext {
    pub(super) fn registry_bytes(&self, candidate: &EvidenceCandidate) -> Option<&[u8]> {
        let key = registry_key(candidate);
        self.registry_bytes.get(&key).map(Vec::as_slice)
    }

    pub(super) fn boot_key(&self, candidate: &EvidenceCandidate) -> Option<[u8; 16]> {
        self.boot_keys
            .get(&candidate.data_source_id)
            .copied()
            .flatten()
    }

    pub(super) fn txlogs(&self, candidate: &EvidenceCandidate) -> (Option<&[u8]>, Option<&[u8]>) {
        let key = registry_key(candidate);
        self.txlog_bytes
            .get(&key)
            .map(|(a, b)| (a.as_deref(), b.as_deref()))
            .unwrap_or((None, None))
    }
}

pub(super) fn preload_registry_context<E: std::fmt::Display>(
    conn: &Connection,
    candidates: &[EvidenceCandidate],
    mut file_reader: impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
    already_extracted: impl Fn(&EvidenceCandidate) -> Result<bool, AnalysisServiceError>,
) -> Result<RegistryPreloadContext, AnalysisServiceError> {
    let mut registry_bytes = HashMap::new();
    let mut txlog_bytes = HashMap::new();
    let mut boot_keys = HashMap::new();
    let mut warnings = Vec::new();

    for candidate in candidates {
        if candidate.category != "Registry" {
            continue;
        }
        if already_extracted(candidate)? {
            continue;
        }
        let mut reader = match file_reader(&candidate.file_id) {
            Ok(reader) => reader,
            Err(err) => {
                warnings.push(format!("{} read failed: {}", candidate.path, err));
                continue;
            }
        };
        let mut bytes = Vec::new();
        if let Err(err) = reader
            .by_ref()
            .take(MAX_ANALYSIS_SOURCE_BYTES as u64)
            .read_to_end(&mut bytes)
        {
            warnings.push(format!("{} read failed: {}", candidate.path, err));
            continue;
        }
        let normalized = normalize_evidence_path(&candidate.path);
        if normalized.ends_with("/windows/system32/config/system") {
            boot_keys.insert(
                candidate.data_source_id.clone(),
                artifacts_windows::extract_boot_key(&bytes),
            );
        }
        registry_bytes.insert(
            (candidate.data_source_id.clone(), normalized.clone()),
            bytes,
        );

        let log1_path = format!("{}.log1", normalized);
        let log2_path = format!("{}.log2", normalized);
        let log1_id = find_file_entry_id_by_path(conn, &candidate.data_source_id, &log1_path);
        let log2_id = find_file_entry_id_by_path(conn, &candidate.data_source_id, &log2_path);
        let log1_bytes = log1_id.and_then(|id| {
            read_file_entry_bytes(
                &mut file_reader,
                &id,
                &candidate.path,
                "LOG1",
                &mut warnings,
            )
        });
        let log2_bytes = log2_id.and_then(|id| {
            read_file_entry_bytes(
                &mut file_reader,
                &id,
                &candidate.path,
                "LOG2",
                &mut warnings,
            )
        });
        txlog_bytes.insert(
            (candidate.data_source_id.clone(), normalized),
            (log1_bytes, log2_bytes),
        );
    }

    Ok(RegistryPreloadContext {
        registry_bytes,
        txlog_bytes,
        boot_keys,
        warnings,
    })
}

fn registry_key(candidate: &EvidenceCandidate) -> (String, String) {
    (
        candidate.data_source_id.clone(),
        normalize_evidence_path(&candidate.path),
    )
}

/// Locate a file entry by its normalized (lower-case, forward-slash) path.
fn find_file_entry_id_by_path(
    conn: &Connection,
    data_source_id: &str,
    normalized_path: &str,
) -> Option<FileEntryId> {
    conn.query_row(
        "SELECT id FROM file_entries \
         WHERE data_source_id = ?1 \
           AND REPLACE(LOWER(path), '\\', '/') = ?2 \
           AND entry_type = 'file' COLLATE NOCASE",
        [data_source_id, normalized_path],
        |row| Ok(FileEntryId(row.get(0)?)),
    )
    .optional()
    .ok()
    .flatten()
}

/// Read the contents of a companion file (e.g. a transaction log) using the
/// same size-bounded reader used for primary evidence sources.
fn read_file_entry_bytes<E: std::fmt::Display>(
    file_reader: &mut impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
    file_id: &FileEntryId,
    hive_path: &str,
    label: &str,
    warnings: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let reader = match file_reader(file_id) {
        Ok(reader) => reader,
        Err(err) => {
            warnings.push(format!("{} {} read failed: {}", hive_path, label, err));
            return None;
        }
    };
    let mut bytes = Vec::new();
    if let Err(err) = reader
        .take(MAX_ANALYSIS_SOURCE_BYTES as u64)
        .read_to_end(&mut bytes)
    {
        warnings.push(format!("{} {} read failed: {}", hive_path, label, err));
        return None;
    }
    Some(bytes)
}
