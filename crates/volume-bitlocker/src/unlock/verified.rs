//! Verified plaintext capability and persisted-package restoration.
use crate::secret::VolumeKeyPackage;
use crate::{PersistedKeyBlob, Result, UnlockedVolume, VolumeIdentity};
use std::sync::Arc;
/// A volume identity paired with immutable cipher state produced only after both
/// AES-CCM authentication checks succeed.
pub struct VerifiedUnlock {
    pub(crate) identity: VolumeIdentity,
    pub(crate) volume: Arc<UnlockedVolume>,
    pub(crate) keys: VolumeKeyPackage,
}

impl VerifiedUnlock {
    /// The metadata copy that produced the verified keys.
    #[must_use]
    pub fn identity(&self) -> &VolumeIdentity {
        &self.identity
    }

    /// Transfers the verified identity and shared plaintext-volume state to the
    /// runtime registry.
    #[must_use]
    pub fn into_unlocked_volume(self) -> (VolumeIdentity, Arc<UnlockedVolume>) {
        (self.identity, self.volume)
    }

    /// Borrows the verified plaintext-volume capability for an additional
    /// read-only validation reader without exposing raw key bytes.
    #[must_use]
    pub fn shared_unlocked_volume(&self) -> Arc<UnlockedVolume> {
        Arc::clone(&self.volume)
    }

    /// Exports the verified key material into the bounded v1 storage envelope.
    /// The raw FVEK remains inaccessible to application and transport layers.
    #[must_use]
    pub fn persisted_key_blob(&self) -> PersistedKeyBlob {
        crate::persisted_key::encode(&self.identity, &self.keys)
    }

    pub(crate) fn from_restored(
        identity: VolumeIdentity,
        volume: Arc<UnlockedVolume>,
        keys: VolumeKeyPackage,
    ) -> Self {
        Self {
            identity,
            volume,
            keys,
        }
    }
}

/// Rebuilds verified runtime state from a persisted key package after strict
/// identity and envelope validation.
pub fn restore_volume_from_persisted_key(
    identity: VolumeIdentity,
    blob: PersistedKeyBlob,
) -> Result<VerifiedUnlock> {
    crate::persisted_key::restore(identity, blob)
}
