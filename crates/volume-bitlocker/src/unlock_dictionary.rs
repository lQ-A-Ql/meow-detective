use std::sync::Arc;

use crate::error::{BitLockerError, Result};
use crate::protector::ProtectorKind;
use crate::reader::UnlockedVolume;
use crate::unlock::{derive_key_package, retain_preferred_error, VerifiedUnlock, VolumeIdentity};

pub(super) fn unlock_identities_with_hash(
    identities: &[VolumeIdentity],
    protector: ProtectorKind,
    protection_code: u16,
    credential_hash: &[u8; 32],
    iterations: u64,
) -> Result<VerifiedUnlock> {
    let mut preferred_error = None;
    for identity in identities {
        match derive_key_package(
            &identity.metadata,
            protector,
            protection_code,
            credential_hash,
            iterations,
        )
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
