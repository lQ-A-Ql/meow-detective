use domain::CaseId;
use volume_bitlocker::{MetadataFingerprint, VerifiedUnlock, VolumeIdentity};

use super::{
    source::{open_registered_plaintext, probe_plaintext_filesystem, BitLockerSource},
    BitLockerServiceError,
};

pub(crate) struct ActivatedUnlock {
    pub identity: VolumeIdentity,
    pub fingerprint: MetadataFingerprint,
    pub plaintext_filesystem: Option<String>,
}

/// Registers verified cipher state and proves it presents a plausible plaintext
/// filesystem. Any probe failure revokes the new runtime state before returning.
pub(crate) fn activate_verified(
    source: &BitLockerSource,
    case_id: &CaseId,
    partition_index: u32,
    preview_runtime: &crate::file_service::PreviewRuntimeRegistry,
    registry: &std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    verified: VerifiedUnlock,
) -> Result<ActivatedUnlock, BitLockerServiceError> {
    let identity = verified.identity().clone();
    let fingerprint = MetadataFingerprint::from_metadata(&identity.metadata);
    registry.register_verified(
        &case_id.0,
        &source.source.id.0,
        partition_index as usize,
        verified,
    )?;
    if let Err(error) = preview_runtime.invalidate_source(&case_id.0, &source.source.id.0) {
        registry.invalidate_partition(&case_id.0, &source.source.id.0, partition_index as usize)?;
        return Err(error.into());
    }
    let plaintext_filesystem = probe_registered(source, case_id, registry);
    match plaintext_filesystem {
        Ok(plaintext_filesystem) => Ok(ActivatedUnlock {
            identity,
            fingerprint,
            plaintext_filesystem,
        }),
        Err(error) => {
            registry.invalidate_partition(
                &case_id.0,
                &source.source.id.0,
                partition_index as usize,
            )?;
            Err(error)
        }
    }
}

fn probe_registered(
    source: &BitLockerSource,
    case_id: &CaseId,
    registry: &std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
) -> Result<Option<String>, BitLockerServiceError> {
    let mut plaintext = open_registered_plaintext(source, case_id, registry)?;
    match probe_plaintext_filesystem(plaintext.as_mut()) {
        Ok(Some(value)) if value == "BitLocker" => Err(BitLockerServiceError::CatalogState(
            "verified plaintext still carries the BitLocker signature".to_string(),
        )),
        result => result,
    }
}
