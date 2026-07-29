use std::{io::Read, path::Path};

use domain::{CaseId, DataSourceId};
use evidence_core::{EvidenceReader, FileSystemReader};
use memory_windows::{
    scan_bitlocker_key_candidates, AesKeyBits, BitLockerKeyCandidate, RawMemoryImage,
};
use rusqlite::Connection;
use transport::dto::BitLockerVolumeStatusDto;
use volume_bitlocker::{
    build_memory_candidate_unlock, read_volume_identities, EncryptionMethod, MemoryCandidateUnlock,
    VerifiedUnlock, VolumeIdentity,
};

use super::{
    source::{open_partition_window, open_source_read_only, BitLockerSource},
    use_cases::{complete_verified_unlock, UnlockContext, UnlockMethod},
    BitLockerRuntimeContext, BitLockerServiceError,
};

const MAX_POOL_ALLOCATIONS_PER_TAG: usize = 8_192;
const MAX_MEMORY_KEY_CANDIDATES: usize = 256;
const MAX_VOLUME_VALIDATION_ATTEMPTS: usize = 4_096;
const UPCASE_PREFIX: [u8; 8] = [0, 0, 1, 0, 2, 0, 3, 0];

/// Recovers and verifies BitLocker FVEK material from a read-only Windows memory image.
///
/// Mathematical AES schedule recognition is only a prefilter. A candidate must
/// independently decrypt a valid NTFS boot sector/MFT and both `$UpCase` and
/// `$Bitmap` before it can enter the runtime registry or secure key storage.
#[allow(clippy::too_many_arguments)]
pub fn unlock_bitlocker_with_memory_image(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    memory_image_path: &Path,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    let context = UnlockContext {
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        runtimes,
    };
    let _read_lease = context
        .runtimes
        .preview_runtime
        .begin_session(case_id, data_source_id)?;
    let source = open_source_read_only(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
    )?;
    let mut window = open_partition_window(&source)?;
    let identities = read_volume_identities(&mut window)?;
    drop(window);

    let mut memory = RawMemoryImage::open(memory_image_path)?;
    let candidates = scan_bitlocker_key_candidates(
        &mut memory,
        MAX_POOL_ALLOCATIONS_PER_TAG,
        MAX_MEMORY_KEY_CANDIDATES,
    )?;
    let verified = verify_candidates(&source, &identities, &candidates)?
        .ok_or(BitLockerServiceError::MemoryKeyNotValidated)?;
    complete_verified_unlock(
        &context,
        &source,
        &identities,
        verified,
        UnlockMethod::MemoryImage,
    )
}

fn verify_candidates(
    source: &BitLockerSource,
    identities: &[VolumeIdentity],
    candidates: &[BitLockerKeyCandidate],
) -> Result<Option<VerifiedUnlock>, BitLockerServiceError> {
    let mut remaining_attempts = MAX_VOLUME_VALIDATION_ATTEMPTS;
    for identity in identities {
        let Some(verified) =
            try_material_combinations(source, identity, candidates, &mut remaining_attempts)?
        else {
            continue;
        };
        return Ok(Some(verified));
    }
    Ok(None)
}

fn try_material_combinations(
    source: &BitLockerSource,
    identity: &VolumeIdentity,
    candidates: &[BitLockerKeyCandidate],
    remaining_attempts: &mut usize,
) -> Result<Option<VerifiedUnlock>, BitLockerServiceError> {
    match identity.metadata.encryption_method {
        EncryptionMethod::Aes128Cbc => try_single_key_candidates(
            source,
            identity,
            candidates,
            AesKeyBits::Aes128,
            remaining_attempts,
        ),
        EncryptionMethod::Aes256Cbc => try_single_key_candidates(
            source,
            identity,
            candidates,
            AesKeyBits::Aes256,
            remaining_attempts,
        ),
        EncryptionMethod::Aes128CbcDiffuser => try_paired_key_candidates(
            source,
            identity,
            candidates,
            AesKeyBits::Aes128,
            remaining_attempts,
        ),
        EncryptionMethod::XtsAes128 => try_paired_key_candidates(
            source,
            identity,
            candidates,
            AesKeyBits::Aes128,
            remaining_attempts,
        ),
        EncryptionMethod::XtsAes256 => try_paired_key_candidates(
            source,
            identity,
            candidates,
            AesKeyBits::Aes256,
            remaining_attempts,
        ),
        EncryptionMethod::Aes256CbcDiffuser | EncryptionMethod::Unknown(_) => Ok(None),
    }
}

fn try_single_key_candidates(
    source: &BitLockerSource,
    identity: &VolumeIdentity,
    candidates: &[BitLockerKeyCandidate],
    bits: AesKeyBits,
    remaining_attempts: &mut usize,
) -> Result<Option<VerifiedUnlock>, BitLockerServiceError> {
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.bits() == bits)
    {
        if !reserve_validation_attempt(remaining_attempts) {
            return Ok(None);
        }
        let pending =
            build_memory_candidate_unlock(identity.clone(), candidate.recovered_key(), None)?;
        if let Some(verified) = validate_candidate(source, pending)? {
            return Ok(Some(verified));
        }
    }
    Ok(None)
}

fn try_paired_key_candidates(
    source: &BitLockerSource,
    identity: &VolumeIdentity,
    candidates: &[BitLockerKeyCandidate],
    bits: AesKeyBits,
    remaining_attempts: &mut usize,
) -> Result<Option<VerifiedUnlock>, BitLockerServiceError> {
    for (left_index, left) in candidates.iter().enumerate() {
        if left.bits() != bits {
            continue;
        }
        for (right_index, right) in candidates.iter().enumerate() {
            if left_index == right_index
                || right.bits() != bits
                || left.pool_physical_address() != right.pool_physical_address()
            {
                continue;
            }
            if !reserve_validation_attempt(remaining_attempts) {
                return Ok(None);
            }
            let pending = build_memory_candidate_unlock(
                identity.clone(),
                left.recovered_key(),
                Some(right.recovered_key()),
            )?;
            if let Some(verified) = validate_candidate(source, pending)? {
                return Ok(Some(verified));
            }
        }
    }
    Ok(None)
}

fn reserve_validation_attempt(remaining_attempts: &mut usize) -> bool {
    let Some(next) = remaining_attempts.checked_sub(1) else {
        return false;
    };
    *remaining_attempts = next;
    true
}

fn validate_candidate(
    source: &BitLockerSource,
    pending: MemoryCandidateUnlock,
) -> Result<Option<VerifiedUnlock>, BitLockerServiceError> {
    let window = open_partition_window(source)?;
    let source_info = window.info().clone();
    let plaintext = match pending.reader(window) {
        Ok(reader) => reader,
        Err(_) => return Ok(None),
    };
    let plaintext =
        crate::bitlocker_runtime::BitLockerEvidenceReader::from_plaintext(plaintext, &source_info);
    let fs = match fs_ntfs::NtfsReader::open(Box::new(plaintext), 0) {
        Ok(reader) => reader,
        Err(_) => return Ok(None),
    };
    if !validate_ntfs_file(&fs, "$UpCase", Some(&UPCASE_PREFIX))
        || !validate_ntfs_file(&fs, "$Bitmap", None)
    {
        return Ok(None);
    }
    Ok(Some(pending.confirm()))
}

fn validate_ntfs_file(
    fs: &fs_ntfs::NtfsReader,
    path: &str,
    expected_prefix: Option<&[u8]>,
) -> bool {
    let mut file = match fs.open_file(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut prefix = [0u8; UPCASE_PREFIX.len()];
    if file.read_exact(&mut prefix).is_err() {
        return false;
    }
    expected_prefix.is_none_or(|expected| prefix.starts_with(expected))
}
