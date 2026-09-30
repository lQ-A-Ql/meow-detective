use std::collections::HashMap;
use std::sync::Arc;
use zeroize::Zeroizing;

use crate::error::{BitLockerError, Result};
use crate::kdf::stretch_key_n;
use crate::protector::ProtectorKind;
use crate::reader::UnlockedVolume;
use crate::unlock::{
    checked_fvek_len, derive_key_package_with_unwrap_key, retain_preferred_error,
    stretch_salt_for_protector, VerifiedUnlock, VolumeIdentity,
};

pub(super) fn unlock_identities_with_hash(
    identities: &[VolumeIdentity],
    protector: ProtectorKind,
    protection_code: u16,
    credential_hash: &[u8; 32],
    iterations: u64,
) -> Result<VerifiedUnlock> {
    unlock_identities_with_stretch(identities, protector, protection_code, |salt| {
        stretch_key_n(credential_hash, salt, iterations)
    })
}

pub(super) fn unlock_identities_with_stretch(
    identities: &[VolumeIdentity],
    protector: ProtectorKind,
    protection_code: u16,
    mut stretch: impl FnMut(&[u8; 16]) -> Zeroizing<[u8; 32]>,
) -> Result<VerifiedUnlock> {
    let mut preferred_error = None;
    let mut stretched_by_salt = HashMap::new();
    for identity in identities {
        if let Err(error) = checked_fvek_len(&identity.metadata) {
            retain_preferred_error(&mut preferred_error, error);
            continue;
        }
        let unwrap_key =
            match stretch_salt_for_protector(&identity.metadata, protector, protection_code) {
                Ok(salt) => stretched_by_salt
                    .entry(salt)
                    .or_insert_with(|| stretch(&salt)),
                Err(error) => {
                    retain_preferred_error(&mut preferred_error, error);
                    continue;
                }
            };
        match derive_key_package_with_unwrap_key(&identity.metadata, protection_code, unwrap_key)
            .and_then(|keys| {
                let volume = UnlockedVolume::new(&identity.metadata, &keys)?;
                Ok((keys, volume))
            }) {
            Ok((keys, volume)) => {
                return Ok(VerifiedUnlock {
                    identity: identity.clone(),
                    volume: Arc::new(volume),
                    keys,
                });
            }
            Err(error) => retain_preferred_error(&mut preferred_error, error),
        }
    }
    Err(
        preferred_error.unwrap_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "no metadata copy produced a verified volume key".to_string(),
        }),
    )
}
