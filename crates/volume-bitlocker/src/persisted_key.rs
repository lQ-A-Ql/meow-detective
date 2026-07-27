//! Versioned binary envelope for verified BitLocker volume keys.
//!
//! This is intentionally not serde. The platform credential is a compact,
//! bounded binary record whose parser rejects unknown versions, mismatched
//! identities, invalid key lengths, truncation, and trailing bytes.

use std::sync::Arc;

use crate::secret::VolumeKeyPackage;
use crate::{
    BitLockerError, MetadataFingerprint, PersistedKeyBlob, Result, UnlockedVolume, VerifiedUnlock,
    VolumeIdentity,
};

const MAGIC: &[u8; 8] = b"MEOWBLK1";
const VERSION: u16 = 1;
const FINGERPRINT_LEN: usize = 32;
const HEADER_LEN: usize = 48;

pub(crate) fn encode(identity: &VolumeIdentity, keys: &VolumeKeyPackage) -> PersistedKeyBlob {
    let fingerprint = MetadataFingerprint::from_metadata(&identity.metadata);
    let fvek = keys.expose_fvek();
    let tweak = keys.expose_tweak().unwrap_or_default();
    let mut bytes = Vec::with_capacity(HEADER_LEN + fvek.len() + tweak.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&identity.metadata.encryption_method_code.to_le_bytes());
    bytes.extend_from_slice(fingerprint.as_str().as_bytes());
    bytes.extend_from_slice(&(fvek.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(tweak.len() as u16).to_le_bytes());
    bytes.extend_from_slice(fvek);
    bytes.extend_from_slice(tweak);
    PersistedKeyBlob::encoded(bytes)
}

pub(crate) fn restore(identity: VolumeIdentity, blob: PersistedKeyBlob) -> Result<VerifiedUnlock> {
    let keys = decode(&identity, blob.expose_for_storage())?;
    let volume = Arc::new(UnlockedVolume::new(&identity.metadata, &keys)?);
    Ok(VerifiedUnlock::from_restored(identity, volume, keys))
}

fn decode(identity: &VolumeIdentity, bytes: &[u8]) -> Result<VolumeKeyPackage> {
    let header = bytes
        .get(..HEADER_LEN)
        .ok_or(BitLockerError::PersistedKeyInvalid {
            reason: "v1 header is truncated",
        })?;
    if header.get(..MAGIC.len()) != Some(MAGIC) {
        return invalid("magic does not identify a v1 key package");
    }
    if read_u16(header, 8)? != VERSION {
        return invalid("version is unsupported");
    }
    if read_u16(header, 10)? != identity.metadata.encryption_method_code {
        return Err(BitLockerError::PersistedKeyMismatch);
    }

    let expected_fingerprint = MetadataFingerprint::from_metadata(&identity.metadata);
    let stored_fingerprint =
        header
            .get(12..12 + FINGERPRINT_LEN)
            .ok_or(BitLockerError::PersistedKeyInvalid {
                reason: "metadata fingerprint is truncated",
            })?;
    if stored_fingerprint != expected_fingerprint.as_str().as_bytes() {
        return Err(BitLockerError::PersistedKeyMismatch);
    }

    let fvek_len = usize::from(read_u16(header, 44)?);
    let tweak_len = usize::from(read_u16(header, 46)?);
    validate_key_lengths(identity, fvek_len, tweak_len)?;
    let expected_len = HEADER_LEN
        .checked_add(fvek_len)
        .and_then(|value| value.checked_add(tweak_len))
        .ok_or(BitLockerError::PersistedKeyInvalid {
            reason: "key lengths overflow the envelope",
        })?;
    if bytes.len() != expected_len {
        return invalid("envelope is truncated or carries trailing bytes");
    }
    let fvek_end = HEADER_LEN + fvek_len;
    let fvek = bytes[HEADER_LEN..fvek_end].to_vec();
    let tweak = (tweak_len != 0).then(|| bytes[fvek_end..expected_len].to_vec());
    Ok(VolumeKeyPackage::new(fvek, tweak))
}

fn validate_key_lengths(
    identity: &VolumeIdentity,
    fvek_len: usize,
    tweak_len: usize,
) -> Result<()> {
    let method = identity.metadata.encryption_method;
    if method.fvek_len() != Some(fvek_len) {
        return invalid("FVEK length does not match the encryption method");
    }
    let expected_tweak_len = if method.uses_diffuser_tweak() { 16 } else { 0 };
    if tweak_len != expected_tweak_len {
        return invalid("tweak length does not match the encryption method");
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(BitLockerError::PersistedKeyInvalid {
            reason: "numeric field is truncated",
        })?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn invalid<T>(reason: &'static str) -> Result<T> {
    Err(BitLockerError::PersistedKeyInvalid { reason })
}

#[cfg(test)]
#[path = "../tests/unit/persisted_key.rs"]
mod tests;
