use super::browser_lsa::{
    derive_sam_prekeys, extend_prekeys_from_lsa_secrets, read_cng_chromekey_file,
    read_elevation_service, PrekeySeed,
};
use super::candidate_processing::{read_candidate_bytes_with_progress, CandidateSource};
use super::reader::CandidateExtractionError;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use artifacts_windows::dpapi::{decrypt_master_key_file, ChromiumDecryptor, DecryptedMasterKey};
use artifacts_windows::{extract_boot_key, extract_sam_fields};
use domain::FileEntryId;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use uuid::Uuid;

const BROWSER_CONTEXT_READ_LIMIT: usize = 32 * 1024 * 1024;

/// Per-analysis browser key context built from evidence hives and Protect files.
pub(super) struct BrowserPreloadContext {
    decryptors: HashMap<(String, String), ChromiumDecryptor>,
    pub(super) warnings: Vec<String>,
}

impl BrowserPreloadContext {
    pub(super) fn empty() -> Self {
        Self {
            decryptors: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    pub(super) fn decryptor_for(
        &self,
        candidate: &EvidenceCandidate,
    ) -> Option<&ChromiumDecryptor> {
        let root = chromium_user_data_root(&normalize_evidence_path(&candidate.path))?;
        self.decryptors
            .get(&(candidate.data_source_id.clone(), root))
    }
}

/// Build browser decryption contexts without making browser parsers aware of
/// databases, evidence readers, or registry layout.
pub(super) fn prepare_browser_preload(
    conn: &Connection,
    candidates: &[EvidenceCandidate],
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<BrowserPreloadContext, AnalysisServiceError> {
    let roots = browser_roots(candidates);
    if roots.is_empty() {
        return Ok(BrowserPreloadContext::empty());
    }

    let mut context = BrowserPreloadContext::empty();
    let data_sources = roots
        .keys()
        .map(|(data_source_id, _)| data_source_id.clone())
        .collect::<HashSet<_>>();
    for data_source_id in data_sources {
        if let Err(error) = prepare_source_context(
            conn,
            &data_source_id,
            roots
                .iter()
                .filter(|((source, _), _)| source == &data_source_id)
                .map(|((_, root), _)| root.clone())
                .collect(),
            cancel_token,
            file_reader,
            &mut context,
        ) {
            context.warnings.push(format!(
                "browser DPAPI context for data source {data_source_id} was not available: {error}"
            ));
        }
    }
    Ok(context)
}

fn prepare_source_context(
    conn: &Connection,
    data_source_id: &str,
    roots: Vec<String>,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    context: &mut BrowserPreloadContext,
) -> Result<(), AnalysisServiceError> {
    let system = locate_by_suffix(conn, data_source_id, "/windows/system32/config/system")
        .ok_or_else(|| AnalysisServiceError::Other("SYSTEM hive was not found".to_string()))?;
    let sam = locate_by_suffix(conn, data_source_id, "/windows/system32/config/sam")
        .ok_or_else(|| AnalysisServiceError::Other("SAM hive was not found".to_string()))?;
    let system_bytes = read_locator(&system, cancel_token, file_reader)?;
    let sam_bytes = read_locator(&sam, cancel_token, file_reader)?;
    let boot_key = extract_boot_key(&system_bytes).ok_or_else(|| {
        AnalysisServiceError::Other("SYSTEM BootKey could not be derived".to_string())
    })?;
    let sam_info = extract_sam_fields(&sam_bytes, "offline SAM", Some(boot_key)).map_err(|_| {
        AnalysisServiceError::Other("SAM user hashes could not be derived".to_string())
    })?;

    let (mut prekeys, user_nt_hashes) = derive_sam_prekeys(sam_info);
    extend_prekeys_from_lsa_secrets(
        conn,
        data_source_id,
        &PrekeySeed {
            boot_key: &boot_key,
            user_nt_hashes: &user_nt_hashes,
        },
        cancel_token,
        file_reader,
        &mut prekeys,
        &mut context.warnings,
    );
    if prekeys.is_empty() {
        return Err(AnalysisServiceError::Other(
            "no usable user DPAPI pre-key was derived".to_string(),
        ));
    }

    let master_keys = load_master_keys(conn, data_source_id, &prekeys, cancel_token, file_reader);
    if master_keys.is_empty() {
        return Err(AnalysisServiceError::Other(
            "no DPAPI master key could be decrypted".to_string(),
        ));
    }

    let cng_key_file = read_cng_chromekey_file(conn, data_source_id, cancel_token, file_reader);
    let elevation_exe = read_elevation_service(conn, data_source_id, cancel_token, file_reader);

    for root in roots {
        ensure_not_cancelled(cancel_token)?;
        let local_state_path = format!("{root}/local state");
        let Some(local_state) = locate_by_suffix(conn, data_source_id, &local_state_path) else {
            context.warnings.push(format!(
                "Chromium Local State was not found for profile root {root}"
            ));
            continue;
        };
        let local_state_bytes = match read_locator(&local_state, cancel_token, file_reader) {
            Ok(bytes) => bytes,
            Err(error) => {
                context.warnings.push(format!(
                    "Chromium Local State could not be read for profile root {root}: {error}"
                ));
                continue;
            }
        };
        let decryptor = build_profile_decryptor(
            &root,
            &local_state_bytes,
            &master_keys,
            cng_key_file.as_deref(),
            elevation_exe.as_deref(),
            &mut context.warnings,
        );
        if let Some(decryptor) = decryptor {
            context
                .decryptors
                .insert((data_source_id.to_string(), root), decryptor);
        }
    }
    Ok(())
}

fn build_profile_decryptor(
    root: &str,
    local_state_bytes: &[u8],
    master_keys: &[DecryptedMasterKey],
    cng_key_file: Option<&[u8]>,
    elevation_exe: Option<&[u8]>,
    warnings: &mut Vec<String>,
) -> Option<ChromiumDecryptor> {
    match ChromiumDecryptor::from_local_state_with_app_bound(
        local_state_bytes,
        master_keys,
        cng_key_file,
        elevation_exe,
    ) {
        Ok(decryptor) => {
            if let Some(error) = decryptor.app_bound_error() {
                warnings.push(format!(
                    "Chromium App-Bound unwrap failed for profile root {root}: {error}"
                ));
            }
            Some(decryptor)
        }
        Err(error) => {
            warnings.push(format!(
                "Chromium Local State could not be unwrapped for profile root {root}: {error}"
            ));
            None
        }
    }
}

fn load_master_keys(
    conn: &Connection,
    data_source_id: &str,
    prekeys: &[[u8; 20]],
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Vec<DecryptedMasterKey> {
    let locators = locate_master_key_files(conn, data_source_id);
    let mut recovered = HashMap::<String, DecryptedMasterKey>::new();
    for locator in locators {
        if ensure_not_cancelled(cancel_token).is_err() {
            break;
        }
        let Ok(bytes) = read_locator(&locator, cancel_token, file_reader) else {
            continue;
        };
        if let Ok(master_key) = decrypt_master_key_file(&bytes, prekeys) {
            recovered
                .entry(master_key.guid.to_ascii_lowercase())
                .or_insert(master_key);
        }
    }
    recovered.into_values().collect()
}

pub(super) fn read_locator(
    locator: &EvidenceCandidate,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<Vec<u8>, AnalysisServiceError> {
    let limit = if locator
        .path
        .to_ascii_lowercase()
        .contains("/microsoft/protect/")
    {
        BROWSER_CONTEXT_READ_LIMIT
    } else {
        MAX_ANALYSIS_SOURCE_BYTES
    };
    read_candidate_bytes_with_progress(locator, limit, cancel_token, file_reader, |_| {}).map_err(
        |error| match error {
            CandidateExtractionError::Cancelled => AnalysisServiceError::Cancelled,
            CandidateExtractionError::Warning(warning) => AnalysisServiceError::Read(warning),
        },
    )
}

fn browser_roots(candidates: &[EvidenceCandidate]) -> HashMap<(String, String), ()> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let normalized = normalize_evidence_path(&candidate.path);
            if !is_chromium_secret_store_path(&normalized) {
                return None;
            }
            chromium_user_data_root(&normalized)
                .map(|root| ((candidate.data_source_id.clone(), root), ()))
        })
        .collect()
}

fn is_chromium_secret_store_path(normalized: &str) -> bool {
    normalized.ends_with("/cookies") || normalized.ends_with("/login data")
}

fn chromium_user_data_root(normalized: &str) -> Option<String> {
    ["/google/chrome/user data/", "/microsoft/edge/user data/"]
        .iter()
        .find_map(|marker| {
            let position = normalized.find(marker)?;
            Some(normalized[..position + marker.len() - 1].to_string())
        })
}

pub(super) fn locate_by_suffix(
    conn: &Connection,
    data_source_id: &str,
    suffix: &str,
) -> Option<EvidenceCandidate> {
    let pattern = format!("%{}", suffix.to_ascii_lowercase());
    let mut statement = conn
        .prepare(
            "SELECT id, path, COALESCE(size, 0), partition_index
             FROM file_entries
             WHERE data_source_id = ?1 AND entry_type = 'file' COLLATE NOCASE
               AND REPLACE(LOWER(path), '\\', '/') LIKE ?2
             ORDER BY LENGTH(path) ASC",
        )
        .ok()?;
    let rows = statement
        .query_map(params![data_source_id, pattern], |row| {
            let id: String = row.get(0)?;
            let path: String = row.get(1)?;
            let size: u64 = row.get(2)?;
            let partition_index = row
                .get::<_, Option<i64>>(3)?
                .and_then(|value| usize::try_from(value).ok());
            Ok((id, path, size, partition_index))
        })
        .ok()?;
    for row in rows.flatten() {
        let (_, path, _, _) = &row;
        if normalize_evidence_path(path).ends_with(suffix) {
            return Some(locator_from_row(data_source_id, row, "BrowserPreload"));
        }
    }
    None
}

fn locate_master_key_files(conn: &Connection, data_source_id: &str) -> Vec<EvidenceCandidate> {
    let Ok(mut statement) = conn.prepare(
        "SELECT id, path, COALESCE(size, 0), partition_index
         FROM file_entries
         WHERE data_source_id = ?1 AND entry_type = 'file' COLLATE NOCASE
           AND REPLACE(LOWER(path), '\\', '/') LIKE '%/microsoft/protect/%'
         ORDER BY path ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map(params![data_source_id], |row| {
        let id: String = row.get(0)?;
        let path: String = row.get(1)?;
        let size: u64 = row.get(2)?;
        let partition_index = row
            .get::<_, Option<i64>>(3)?
            .and_then(|value| usize::try_from(value).ok());
        Ok((id, path, size, partition_index))
    }) else {
        return Vec::new();
    };
    rows.flatten()
        .filter(|(_, path, _, _)| is_master_key_path(path))
        .map(|row| locator_from_row(data_source_id, row, "BrowserPreload"))
        .collect()
}

fn is_master_key_path(path: &str) -> bool {
    let normalized = normalize_evidence_path(path);
    let Some(name) = normalized.rsplit('/').next() else {
        return false;
    };
    if Uuid::parse_str(name).is_err() || !normalized.contains("/microsoft/protect/") {
        return false;
    }
    let parent_is_sid = normalized
        .split('/')
        .rev()
        .nth(1)
        .is_some_and(|sid| sid.starts_with("s-1-"));
    let system_protect = normalized.contains("/microsoft/protect/s-1-5-18/user/")
        || normalized.contains("/microsoft/protect/s-1-5-18/machine/");
    parent_is_sid || system_protect
}

pub(super) fn locator_from_row(
    data_source_id: &str,
    row: (String, String, u64, Option<usize>),
    category: &str,
) -> EvidenceCandidate {
    let (id, path, size, partition_index) = row;
    EvidenceCandidate {
        file_id: FileEntryId(id),
        data_source_id: data_source_id.to_string(),
        partition_index,
        path,
        size,
        content_identity: format!("browser-preload:{data_source_id}:{size}"),
        evidence_kind: "browser_preload".to_string(),
        parser: "browser.dpapi".to_string(),
        category: category.to_string(),
    }
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), AnalysisServiceError> {
    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        Err(AnalysisServiceError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/browser_preload.rs"]
mod tests;
