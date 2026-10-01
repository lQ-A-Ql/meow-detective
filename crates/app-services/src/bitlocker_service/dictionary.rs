//! Password-dictionary orchestration for BitLocker volumes.
//!
//! Candidate strings are kept in a zeroizing [`Passphrase`] for the duration
//! of one attempt.  This module deliberately returns only progress and the
//! normal verified-unlock status; a matching password never crosses the
//! service boundary.

use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::BitLockerDictionaryBackendDto;
use volume_bitlocker::{read_volume_identities, BitLockerError, ProtectorKind};

mod dictionary_audit;
mod dictionary_input;

pub(super) use dictionary_audit::record_dictionary_outcome;
pub(super) use dictionary_input::{read_candidate_batch, DictionaryBatch};

use super::{
    context::BitLockerRuntimeContext,
    dictionary_backend::DictionaryBackend,
    dictionary_identity::{open_and_fingerprint, DictionaryIdentity},
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
/// Keep the in-flight candidate buffer bounded even when a dictionary contains
/// many long lines.  The line limit remains the upper bound for one candidate.
pub const MAX_DICTIONARY_BATCH_BYTES: usize = 4 * 1024 * 1024;

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
    pub backend: BitLockerDictionaryBackendDto,
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
    let _read_lease = request
        .runtimes
        .preview_runtime
        .begin_session(request.case_id, request.data_source_id)?;
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
    let workers = DictionaryBackend::new(request.backend, &identities)?;
    let batch_size = workers.batch_size();
    let mut progress = DictionaryAttackProgress {
        tested_candidates: 0,
        bytes_processed: 0,
        total_bytes: dictionary_size,
    };
    loop {
        let candidates = match read_candidate_batch(
            &mut reader,
            request.cancel_token,
            &mut progress.bytes_processed,
            batch_size,
        )? {
            DictionaryBatch::Cancelled => {
                return Ok(DictionaryAttackOutcome::Cancelled { progress });
            }
            DictionaryBatch::Empty => continue,
            DictionaryBatch::End => return Ok(DictionaryAttackOutcome::Exhausted { progress }),
            DictionaryBatch::Candidates(candidates) => candidates,
        };
        on_progress(progress);
        let batch = workers.try_batch(&identities, candidates, request.cancel_token)?;
        progress.tested_candidates = progress
            .tested_candidates
            .saturating_add(batch.attempted as u64);
        *tested_candidates = progress.tested_candidates;
        match batch.verified? {
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
            None => {
                on_progress(progress);
                if request.cancel_token.load(Ordering::Acquire) {
                    return Ok(DictionaryAttackOutcome::Cancelled { progress });
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_dictionary.rs"]
mod tests;
