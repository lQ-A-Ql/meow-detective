//! Unlock with a structurally recovered, still-opaque VMK.

use std::io::{Read, Seek};
use std::sync::Arc;

use crate::error::{BitLockerError, Result};
use crate::metadata::FveMetadata;
use crate::reader::UnlockedVolume;
use crate::secret::{RecoveredVmk, VolumeKeyPackage};
use crate::unlock::{
    derive_key_package_from_vmk_bytes, read_volume_identities, retain_preferred_error,
    VerifiedUnlock,
};

/// Unlocks a volume with a structurally recovered and still-opaque VMK.
///
/// The VMK must come from a volume-bound memory structure. The wrapped FVEK's
/// AES-CCM tag remains the cryptographic volume oracle before a verified unlock
/// can be produced.
pub fn unlock_volume_with_recovered_vmk<R: Read + Seek>(
    reader: &mut R,
    vmk: &RecoveredVmk,
) -> Result<VerifiedUnlock> {
    let identities = read_volume_identities(reader)?;
    let mut preferred_error = None;
    for identity in identities {
        match derive_key_package_from_vmk(&identity.metadata, vmk).and_then(|keys| {
            let volume = UnlockedVolume::new(&identity.metadata, &keys)?;
            Ok((keys, volume))
        }) {
            Ok((keys, volume)) => {
                return Ok(VerifiedUnlock::from_restored(
                    identity,
                    Arc::new(volume),
                    keys,
                ));
            }
            Err(error) => retain_preferred_error(&mut preferred_error, error),
        }
    }
    Err(
        preferred_error.unwrap_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "no metadata copy authenticated the recovered VMK".to_string(),
        }),
    )
}

fn derive_key_package_from_vmk(
    metadata: &FveMetadata,
    vmk: &RecoveredVmk,
) -> Result<VolumeKeyPackage> {
    let method = metadata.encryption_method;
    let fvek_len = method
        .fvek_len()
        .ok_or(BitLockerError::UnsupportedEncryptionMethod {
            code: metadata.encryption_method_code,
            label: method.label(),
        })?;
    derive_key_package_from_vmk_bytes(metadata, vmk.expose_for_recovery(), fvek_len)
}
