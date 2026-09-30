//! Password-dictionary orchestration for BitLocker volumes.
//!
//! Candidate strings are kept in a zeroizing [`Passphrase`] for the duration
//! of one attempt.  This module deliberately returns only progress and the
//! normal verified-unlock status; a matching password never crosses the
//! service boundary.

use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use zeroize::Zeroizing;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::ServiceErrorCategory;
use volume_bitlocker::{
    read_volume_identities, BitLockerError, MetadataFingerprint, ProtectorKind,
};

use super::{
    audit::{self, BitLockerAudit},
    context::BitLockerRuntimeContext,
    dictionary_identity::{open_and_fingerprint, DictionaryIdentity},
    dictionary_parallel::{worker_count, DictionaryWorkers},
    error::BitLockerServiceError,
    source::{open_partition_window, open_source_read_only},
    use_cases::{complete_verified_unlock, UnlockContext, UnlockMethod},
};

/// Keep malformed dictionary files from causing an unbounded allocation for a
/// single line.  Passwords longer than this are rejected as dictionary input.
pub const MAX_DICTIONARY_LINE_BYTES: usize = 1024 * 1024;
/// A dictionary is an investigator-selected input, but still receives a hard
/// bound so an accidental multi-gigabyte file cannot exhaust the worker.
pub const MAX_DICTIONARY_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DictionaryAttackProgress {
    pub tested_candidates: u64,
    pub bytes_processed: u64,
    pub total_bytes: u64,
}

pub struct DictionaryAttackRequest<'a> {
    pub case_conn: &'a Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub data_source_id: &'a DataSourceId,
    pub partition_index: u32,
    pub dictionary_path: &'a Path,
    pub runtimes: BitLockerRuntimeContext<'a>,
    pub cancel_token: &'a AtomicBool,
}

pub enum DictionaryAttackOutcome {
    Found { progress: DictionaryAttackProgress },
    Exhausted { progress: DictionaryAttackProgress },
    Cancelled { progress: DictionaryAttackProgress },
}

/// Validate the investigator-selected dictionary before scheduling work.
pub fn validate_dictionary_path(path: &Path) -> Result<u64, BitLockerServiceError> {
    let metadata = std::fs::metadata(path).map_err(BitLockerServiceError::DictionaryRead)?;
    if !metadata.is_file() {
        return Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary path is not a file",
        });
    }
    if metadata.len() > MAX_DICTIONARY_BYTES {
        return Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary file exceeds the size limit",
        });
    }
    Ok(metadata.len())
}

/// Try each UTF-8 line in a password dictionary against one immutable set of
/// BitLocker metadata copies.  The evidence and case database remain read-only
/// until a candidate authenticates; then the normal activation/persistence path
/// is used exactly once.
pub fn try_password_dictionary(
    request: DictionaryAttackRequest<'_>,
    mut on_progress: impl FnMut(DictionaryAttackProgress),
) -> Result<DictionaryAttackOutcome, BitLockerServiceError> {
    let mut identity = None;
    let mut tested_candidates = 0;
    let result = try_password_dictionary_inner(
        &request,
        &mut identity,
        &mut tested_candidates,
        &mut on_progress,
    );
    record_dictionary_outcome(&request, &result, identity.as_ref(), tested_candidates);
    result
}

fn try_password_dictionary_inner(
    request: &DictionaryAttackRequest<'_>,
    identity: &mut Option<DictionaryIdentity>,
    tested_candidates: &mut u64,
    on_progress: &mut impl FnMut(DictionaryAttackProgress),
) -> Result<DictionaryAttackOutcome, BitLockerServiceError> {
    let Some((file, dictionary_identity)) =
        open_and_fingerprint(request.dictionary_path, request.cancel_token)?
    else {
        return Ok(DictionaryAttackOutcome::Cancelled {
            progress: DictionaryAttackProgress {
                tested_candidates: 0,
                bytes_processed: 0,
                total_bytes: 0,
            },
        });
    };
    let dictionary_size = dictionary_identity.size;
    *identity = Some(dictionary_identity);
    let source = open_source_read_only(
        request.case_conn,
        request.case_root,
        request.case_id,
        request.data_source_id,
        request.partition_index,
    )?;
    let mut window = open_partition_window(&source)?;
    let identities = read_volume_identities(&mut window)?;
    if !identities.iter().any(|identity| {
        identity
            .metadata
            .protector_inventory()
            .protectors()
            .contains(&ProtectorKind::Password)
    }) {
        return Err(BitLockerError::UnsupportedProtector {
            found: "no password protector".to_string(),
        }
        .into());
    }
    let mut reader = BufReader::new(file);
    let batch_size = worker_count(1024).saturating_mul(4).max(1);
    let workers = DictionaryWorkers::new(batch_size)?;
    let mut progress = DictionaryAttackProgress {
        tested_candidates: 0,
        bytes_processed: 0,
        total_bytes: dictionary_size,
    };
    loop {
        if request.cancel_token.load(Ordering::Acquire) {
            return Ok(DictionaryAttackOutcome::Cancelled { progress });
        }
        let mut candidates = Vec::with_capacity(batch_size);
        while candidates.len() < batch_size {
            let Some(raw_line) = read_bounded_line(&mut reader)? else {
                break;
            };
            progress.bytes_processed =
                checked_bytes_processed(progress.bytes_processed, raw_line.len())?;
            let candidate = decode_candidate(raw_line)?;
            if !candidate.is_empty() {
                candidates.push(candidate);
            }
        }
        if candidates.is_empty() {
            return Ok(DictionaryAttackOutcome::Exhausted { progress });
        }
        progress.tested_candidates = progress
            .tested_candidates
            .saturating_add(candidates.len() as u64);
        *tested_candidates = progress.tested_candidates;
        match workers.try_batch(&identities, candidates, request.cancel_token)? {
            Some(verified) => {
                if request.cancel_token.load(Ordering::Acquire) {
                    return Ok(DictionaryAttackOutcome::Cancelled { progress });
                }
                let context = UnlockContext {
                    case_conn: request.case_conn,
                    case_root: request.case_root,
                    case_id: request.case_id,
                    data_source_id: request.data_source_id,
                    partition_index: request.partition_index,
                    runtimes: request.runtimes,
                };
                complete_verified_unlock(
                    &context,
                    &source,
                    &identities,
                    verified,
                    UnlockMethod::Password,
                    None,
                )?;
                return Ok(DictionaryAttackOutcome::Found { progress });
            }
            None => on_progress(progress),
        }
    }
}

fn record_dictionary_outcome(
    request: &DictionaryAttackRequest<'_>,
    result: &Result<DictionaryAttackOutcome, BitLockerServiceError>,
    identity: Option<&DictionaryIdentity>,
    tested_candidates: u64,
) {
    let (outcome, code) = match result {
        Ok(DictionaryAttackOutcome::Found { .. }) => ("success", None),
        Ok(DictionaryAttackOutcome::Exhausted { .. }) => ("exhausted", None),
        Ok(DictionaryAttackOutcome::Cancelled { .. }) => ("cancelled", None),
        Err(error) => ("failed", error.code()),
    };
    let fingerprint = read_dictionary_fingerprint(request);
    let tested_candidates = dictionary_tested_candidates(result, tested_candidates);
    let extra = dictionary_input_details(identity, tested_candidates);
    audit::record(
        request.case_conn,
        BitLockerAudit {
            case_id: &request.case_id.0,
            data_source_id: &request.data_source_id.0,
            partition_index: request.partition_index,
            metadata_fingerprint: fingerprint.as_deref(),
            operation: "passwordDictionary",
            outcome,
            error_code: code,
            extra_details: Some(&extra),
        },
    );
}

fn dictionary_input_details(
    identity: Option<&DictionaryIdentity>,
    tested_candidates: u64,
) -> serde_json::Value {
    let mut details = serde_json::Map::new();
    details.insert(
        "testedCandidates".to_string(),
        serde_json::json!(tested_candidates),
    );
    if let Some(identity) = identity {
        details.insert(
            "dictionarySize".to_string(),
            serde_json::json!(identity.size),
        );
        details.insert(
            "dictionarySha256".to_string(),
            serde_json::json!(identity.sha256),
        );
    }
    serde_json::Value::Object(details)
}

fn dictionary_tested_candidates(
    result: &Result<DictionaryAttackOutcome, BitLockerServiceError>,
    fallback: u64,
) -> u64 {
    match result {
        Ok(DictionaryAttackOutcome::Found { progress })
        | Ok(DictionaryAttackOutcome::Exhausted { progress })
        | Ok(DictionaryAttackOutcome::Cancelled { progress }) => progress.tested_candidates,
        Err(_) => fallback,
    }
}

fn read_dictionary_fingerprint(request: &DictionaryAttackRequest<'_>) -> Option<String> {
    let source = open_source_read_only(
        request.case_conn,
        request.case_root,
        request.case_id,
        request.data_source_id,
        request.partition_index,
    )
    .ok()?;
    let mut window = open_partition_window(&source).ok()?;
    let identities = read_volume_identities(&mut window).ok()?;
    identities.first().map(|identity| {
        MetadataFingerprint::from_metadata(&identity.metadata)
            .as_str()
            .to_string()
    })
}

fn checked_bytes_processed(current: u64, line_len: usize) -> Result<u64, BitLockerServiceError> {
    let additional =
        u64::try_from(line_len).map_err(|_| BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary file exceeds the size limit",
        })?;
    let next = current
        .checked_add(additional)
        .ok_or(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary file exceeds the size limit",
        })?;
    if next > MAX_DICTIONARY_BYTES {
        return Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary file exceeds the size limit",
        });
    }
    Ok(next)
}

fn decode_candidate(
    raw_line: Zeroizing<Vec<u8>>,
) -> Result<Zeroizing<String>, BitLockerServiceError> {
    let mut raw_line = raw_line;
    if raw_line.last() == Some(&b'\n') {
        raw_line.pop();
        if raw_line.last() == Some(&b'\r') {
            raw_line.pop();
        }
    }
    let mut candidate = Zeroizing::new(
        std::str::from_utf8(&raw_line)
            .map_err(|_| BitLockerServiceError::DictionaryInvalid {
                reason: "dictionary contains invalid UTF-8",
            })?
            .to_owned(),
    );
    if candidate.starts_with('\u{feff}') {
        candidate.drain(..'\u{feff}'.len_utf8());
    }
    Ok(candidate)
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
) -> Result<Option<Zeroizing<Vec<u8>>>, BitLockerServiceError> {
    let mut line = Zeroizing::new(Vec::new());
    loop {
        let chunk = reader
            .fill_buf()
            .map_err(BitLockerServiceError::DictionaryRead)?;
        if chunk.is_empty() {
            return Ok((!line.is_empty()).then(|| std::mem::take(&mut line)));
        }
        if let Some(newline) = chunk.iter().position(|byte| *byte == b'\n') {
            let take = newline + 1;
            if line.len().saturating_add(take) > MAX_DICTIONARY_LINE_BYTES {
                return Err(BitLockerServiceError::DictionaryInvalid {
                    reason: "dictionary line exceeds the size limit",
                });
            }
            line.extend_from_slice(&chunk[..take]);
            reader.consume(take);
            return Ok(Some(std::mem::take(&mut line)));
        }
        if line.len().saturating_add(chunk.len()) > MAX_DICTIONARY_LINE_BYTES {
            return Err(BitLockerServiceError::DictionaryInvalid {
                reason: "dictionary line exceeds the size limit",
            });
        }
        let consumed = chunk.len();
        line.extend_from_slice(chunk);
        reader.consume(consumed);
    }
}

impl From<io::Error> for BitLockerServiceError {
    fn from(error: io::Error) -> Self {
        Self::DictionaryRead(error)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_dictionary.rs"]
mod tests;
