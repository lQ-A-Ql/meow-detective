use std::sync::Arc;

use zeroize::Zeroize;

use crate::{
    secret::VolumeKeyPackage, BitLockerReader, Result, UnlockedVolume, VerifiedUnlock,
    VolumeIdentity,
};

/// One AES key recovered from volatile memory.
///
/// This transport stays opaque outside this crate: no byte accessor, `Debug`,
/// `Clone`, or serialization implementation is provided. The buffer is zeroized
/// on drop, including failed candidate construction paths.
pub struct RecoveredAesKey {
    bytes: Vec<u8>,
}

impl RecoveredAesKey {
    pub fn new(mut bytes: Vec<u8>) -> Result<Self> {
        if !matches!(bytes.len(), 16 | 32) {
            bytes.zeroize();
            return Err(crate::BitLockerError::MetadataUnreadable {
                reason: "a recovered AES key must contain 16 or 32 bytes".to_string(),
            });
        }
        Ok(Self { bytes })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Makes a deliberate bounded copy for one alternate candidate pairing.
    #[must_use]
    pub fn copy_for_validation(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
        }
    }

    fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }

    fn expose_for_assembly(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for RecoveredAesKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// Secret-bearing cipher state built from memory-recovered key material.
///
/// This is deliberately not a [`VerifiedUnlock`]. The application layer must
/// validate independent filesystem structures before calling [`Self::confirm`].
/// It has no `Debug`, `Clone`, or serialization implementation.
pub struct MemoryCandidateUnlock {
    identity: VolumeIdentity,
    volume: Arc<UnlockedVolume>,
    keys: VolumeKeyPackage,
}

impl MemoryCandidateUnlock {
    /// Builds an isolated plaintext reader for one validation pass.
    pub fn reader<R>(&self, evidence: R) -> Result<BitLockerReader<R>>
    where
        R: std::io::Read + std::io::Seek,
    {
        BitLockerReader::new(self.volume.clone(), evidence)
    }

    /// Promotes the candidate only after the caller has completed its documented
    /// volume-bound validation oracles.
    #[must_use]
    pub fn confirm(self) -> VerifiedUnlock {
        VerifiedUnlock::from_restored(self.identity, self.volume, self.keys)
    }
}

/// Builds secret-bearing candidate cipher state from raw memory key bytes.
///
/// The vectors are moved directly into zeroizing storage. Length and encryption
/// method compatibility are enforced by [`UnlockedVolume::new`].
pub fn build_memory_candidate_unlock(
    identity: VolumeIdentity,
    first: RecoveredAesKey,
    second: Option<RecoveredAesKey>,
) -> Result<MemoryCandidateUnlock> {
    let (fvek, tweak) = assemble_key_package(identity.metadata.encryption_method, first, second)?;
    let keys = VolumeKeyPackage::new(fvek, tweak);
    let volume = Arc::new(UnlockedVolume::new(&identity.metadata, &keys)?);
    Ok(MemoryCandidateUnlock {
        identity,
        volume,
        keys,
    })
}

fn assemble_key_package(
    method: crate::EncryptionMethod,
    first: RecoveredAesKey,
    second: Option<RecoveredAesKey>,
) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
    use crate::EncryptionMethod;

    match method {
        EncryptionMethod::XtsAes128 | EncryptionMethod::XtsAes256 => {
            let second = second.ok_or_else(|| crate::BitLockerError::MetadataUnreadable {
                reason: "an XTS memory candidate requires data and tweak AES keys".to_string(),
            })?;
            if first.len() != second.len() {
                return Err(crate::BitLockerError::MetadataUnreadable {
                    reason: "XTS memory candidate keys have different lengths".to_string(),
                });
            }
            let mut fvek = Vec::with_capacity(first.len() + second.len());
            fvek.extend_from_slice(first.expose_for_assembly());
            fvek.extend_from_slice(second.expose_for_assembly());
            Ok((fvek, None))
        }
        EncryptionMethod::Aes128CbcDiffuser | EncryptionMethod::Aes256CbcDiffuser => {
            let tweak = second.ok_or_else(|| crate::BitLockerError::MetadataUnreadable {
                reason: "a diffuser memory candidate requires a separate tweak key".to_string(),
            })?;
            Ok((first.into_bytes(), Some(tweak.into_bytes())))
        }
        EncryptionMethod::Aes128Cbc | EncryptionMethod::Aes256Cbc => {
            if second.is_some() {
                return Err(crate::BitLockerError::MetadataUnreadable {
                    reason: "a CBC memory candidate carries an unexpected second key".to_string(),
                });
            }
            Ok((first.into_bytes(), None))
        }
        EncryptionMethod::Unknown(_) => Err(crate::BitLockerError::UnsupportedEncryptionMethod {
            code: method.code(),
            label: method.label(),
        }),
    }
}

#[cfg(test)]
#[path = "../tests/unit/memory_candidate.rs"]
mod tests;
