use std::{io::Read, path::Path};

use domain::{CaseId, DataSourceId};
use evidence_core::{EvidenceReader, FileSystemReader};
use memory_windows::{
    recover_vmks_structurally, resolve_profile_for_image, TargetedKernelSearchLimits,
};
use rusqlite::Connection;
use transport::dto::{BitLockerVolumeStatusDto, RecoveryPasswordReconstructionDto};
use volume_bitlocker::{
    recover_recovery_password, recovery_password_protectors, unlock_volume_with_recovered_vmk,
    FveMetadata, RecoveredVmk, VerifiedUnlock, VolumeIdentity,
};

use super::{
    audit::{self, BitLockerAudit},
    source::{open_partition_window, open_source_read_only, BitLockerSource},
    use_cases::{complete_verified_unlock, UnlockContext, UnlockMethod},
    BitLockerRuntimeContext, BitLockerServiceError,
};

const UPCASE_PREFIX: [u8; 8] = [0, 0, 1, 0, 2, 0, 3, 0];

/// Recovers and volume-authenticates a VMK from a raw memory image.
///
/// Runtime discovery resolves the ntoskrnl CodeView GUID against the embedded
/// PDB symbol registry (unknown builds fail closed), then follows exact kernel
/// objects and a registry-bound FVEVol client extension. It never falls back
/// to pool tags, writable-section roots, pointer graphs, AES schedules, or key
/// pairing.
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
    let identities = read_identities(&source)?;
    let target = identities
        .first()
        .ok_or(BitLockerServiceError::MemoryKeyNotValidated)?;
    let profile = resolve_profile_for_image(memory_image_path)?;
    let recovery = recover_vmks_structurally(
        memory_image_path,
        &profile,
        target.metadata.volume_guid,
        TargetedKernelSearchLimits::default(),
    )?;
    let reads = recovery.physical_reads();
    tracing::info!(
        profile_id = recovery.profile_id(),
        build_id = recovery.build_id(),
        recovered_vmk_count = recovery.recovered_vmk_count(),
        keyring_datasets_examined = recovery.keyring_datasets_examined(),
        device_contexts_examined = recovery.devices_examined(),
        vmk_datum_pointers_examined = recovery.datum_pointers_examined(),
        physical_read_operations = reads.operations,
        physical_bytes_read = reads.bytes_read,
        "completed structural BitLocker memory recovery"
    );
    let vmks = recovery.into_vmks();
    // Try every recovered VMK generation for the recovery-password
    // reconstruction: the VMK that wrapped the protector's reverse datum is
    // not necessarily the active, volume-unlocking one.
    let reveal = reconstruct_recovery_password_from_any(&identities, &vmks);
    let verified = select_verified_unlock(&source, vmks)?;
    audit_reconstruction(
        &context,
        &verified.identity().metadata,
        reveal.status.as_str(),
    );
    complete_verified_unlock(
        &context,
        &source,
        &identities,
        verified,
        UnlockMethod::MemoryImage,
        Some(reveal),
    )
}

fn read_identities(source: &BitLockerSource) -> Result<Vec<VolumeIdentity>, BitLockerServiceError> {
    let mut window = open_partition_window(source)?;
    let identities = volume_bitlocker::read_volume_identities(&mut window)?;
    Ok(identities)
}

fn select_verified_unlock(
    source: &BitLockerSource,
    vmks: Vec<RecoveredVmk>,
) -> Result<VerifiedUnlock, BitLockerServiceError> {
    let mut matching = None;
    for vmk in vmks {
        let Ok(verified) = unlock_and_validate(source, &vmk) else {
            continue;
        };
        if matching.replace(verified).is_some() {
            return Err(BitLockerServiceError::MemoryKeyNotValidated);
        }
    }
    matching.ok_or(BitLockerServiceError::MemoryKeyNotValidated)
}

/// Attempts the recovery-password reconstruction with every recovered VMK
/// across every metadata copy; the first authenticated VMK wins. Falls back
/// to the canonical `unavailable` result when none authenticates.
fn reconstruct_recovery_password_from_any(
    identities: &[VolumeIdentity],
    vmks: &[RecoveredVmk],
) -> RecoveryPasswordReconstructionDto {
    for identity in identities {
        for vmk in vmks {
            let reveal = reconstruct_recovery_password(&identity.metadata, vmk);
            if reveal.status == "recovered" {
                return reveal;
            }
        }
    }
    match (identities.first(), vmks.first()) {
        (Some(identity), Some(vmk)) => reconstruct_recovery_password(&identity.metadata, vmk),
        _ => RecoveryPasswordReconstructionDto {
            status: "unavailable".to_string(),
            password: None,
            volume_guid: None,
            protector_guid: None,
            reverse_datum_fingerprint: None,
            reason: Some("no VMK was recovered from the memory image".to_string()),
        },
    }
}

/// Reconstructs the numerical recovery password from the volume-authenticated
/// VMK. A valid VMK does not guarantee reconstruction: the recovery
/// protector's reverse datum may have been wrapped under an older VMK
/// generation, in which case the result is an explicit `unavailable`.
fn reconstruct_recovery_password(
    metadata: &FveMetadata,
    vmk: &RecoveredVmk,
) -> RecoveryPasswordReconstructionDto {
    let unavailable = |reason: &str| RecoveryPasswordReconstructionDto {
        status: "unavailable".to_string(),
        password: None,
        volume_guid: None,
        protector_guid: None,
        reverse_datum_fingerprint: None,
        reason: Some(reason.to_string()),
    };
    let Ok(protectors) = recovery_password_protectors(metadata) else {
        return unavailable("no recovery-password protector in the FVE metadata");
    };
    if protectors.is_empty() {
        return unavailable("no recovery-password protector in the FVE metadata");
    }
    for protector in protectors {
        if let Ok(recovered) = recover_recovery_password(metadata, protector, vmk) {
            let provenance = recovered.provenance();
            return RecoveryPasswordReconstructionDto {
                status: "recovered".to_string(),
                password: Some(
                    recovered
                        .password()
                        .expose_for_authorized_reveal()
                        .to_string(),
                ),
                volume_guid: Some(provenance.volume_guid().to_string()),
                protector_guid: Some(provenance.protector_guid().to_string()),
                reverse_datum_fingerprint: Some(provenance.reverse_datum_fingerprint().to_string()),
                reason: None,
            };
        }
    }
    unavailable("the active VMK does not authenticate any recovery protector's reverse datum")
}

fn audit_reconstruction(context: &UnlockContext<'_>, metadata: &FveMetadata, outcome: &str) {
    let fingerprint = volume_bitlocker::MetadataFingerprint::from_metadata(metadata);
    audit::record(
        context.case_conn,
        BitLockerAudit {
            case_id: &context.case_id.0,
            data_source_id: &context.data_source_id.0,
            partition_index: context.partition_index,
            metadata_fingerprint: Some(fingerprint.as_str()),
            operation: "recoveryPasswordReconstruction",
            outcome,
            error_code: None,
        },
    );
}

fn unlock_and_validate(
    source: &BitLockerSource,
    vmk: &RecoveredVmk,
) -> Result<VerifiedUnlock, BitLockerServiceError> {
    let mut window = open_partition_window(source)?;
    let verified = unlock_volume_with_recovered_vmk(&mut window, vmk)?;
    let source_info = window.info().clone();
    let plaintext = volume_bitlocker::BitLockerReader::new(
        verified.shared_unlocked_volume(),
        open_partition_window(source)?,
    )?;
    let plaintext =
        crate::bitlocker_runtime::BitLockerEvidenceReader::from_plaintext(plaintext, &source_info);
    let fs = fs_ntfs::NtfsReader::open(Box::new(plaintext), 0)
        .map_err(BitLockerServiceError::PlaintextValidation)?;
    if !validate_ntfs_file(&fs, "$UpCase", Some(&UPCASE_PREFIX))
        || !validate_ntfs_file(&fs, "$Bitmap", None)
    {
        return Err(BitLockerServiceError::MemoryKeyNotValidated);
    }
    Ok(verified)
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
