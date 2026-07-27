use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use volume_bitlocker::{MetadataFingerprint, UnlockedVolume, VerifiedUnlock, VolumeIdentity};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BitLockerVolumeScope {
    case_id: String,
    data_source_id: String,
    partition_index: usize,
    metadata_fingerprint: MetadataFingerprint,
}

impl BitLockerVolumeScope {
    fn new(
        case_id: impl Into<String>,
        data_source_id: impl Into<String>,
        partition_index: usize,
        metadata_fingerprint: MetadataFingerprint,
    ) -> Self {
        Self {
            case_id: case_id.into(),
            data_source_id: data_source_id.into(),
            partition_index,
            metadata_fingerprint,
        }
    }

    #[must_use]
    pub fn metadata_fingerprint(&self) -> &MetadataFingerprint {
        &self.metadata_fingerprint
    }
}

pub struct RegisteredBitLockerVolume {
    scope: BitLockerVolumeScope,
    volume: Arc<UnlockedVolume>,
}

impl RegisteredBitLockerVolume {
    #[must_use]
    pub fn scope(&self) -> &BitLockerVolumeScope {
        &self.scope
    }

    #[must_use]
    pub fn volume(&self) -> Arc<UnlockedVolume> {
        self.volume.clone()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BitLockerRuntimeError {
    #[error("BitLocker volume is locked")]
    Locked,
    #[error("BitLocker runtime registry is unavailable")]
    RegistryUnavailable,
    #[error("BitLocker volume window is invalid: {0}")]
    InvalidWindow(#[source] std::io::Error),
    #[error(transparent)]
    Volume(#[from] volume_bitlocker::BitLockerError),
}

#[derive(Default)]
pub struct BitLockerUnlockRegistry {
    volumes: Mutex<HashMap<BitLockerVolumeScope, Arc<UnlockedVolume>>>,
}

impl BitLockerUnlockRegistry {
    pub fn register_verified(
        &self,
        case_id: &str,
        data_source_id: &str,
        partition_index: usize,
        verified: VerifiedUnlock,
    ) -> Result<RegisteredBitLockerVolume, BitLockerRuntimeError> {
        let fingerprint = MetadataFingerprint::from_metadata(&verified.identity().metadata);
        let scope =
            BitLockerVolumeScope::new(case_id, data_source_id, partition_index, fingerprint);
        let (_, volume) = verified.into_unlocked_volume();
        let mut volumes = self.lock()?;
        volumes.retain(|candidate, _| {
            candidate.case_id != scope.case_id
                || candidate.data_source_id != scope.data_source_id
                || candidate.partition_index != scope.partition_index
        });
        volumes.insert(scope.clone(), volume.clone());
        Ok(RegisteredBitLockerVolume { scope, volume })
    }

    pub fn resolve_for_identities(
        &self,
        case_id: &str,
        data_source_id: &str,
        partition_index: usize,
        identities: &[VolumeIdentity],
    ) -> Result<RegisteredBitLockerVolume, BitLockerRuntimeError> {
        let volumes = self.lock()?;
        for identity in identities {
            let scope = BitLockerVolumeScope::new(
                case_id,
                data_source_id,
                partition_index,
                MetadataFingerprint::from_metadata(&identity.metadata),
            );
            if let Some(volume) = volumes.get(&scope) {
                return Ok(RegisteredBitLockerVolume {
                    scope,
                    volume: volume.clone(),
                });
            }
        }
        Err(BitLockerRuntimeError::Locked)
    }

    pub fn invalidate_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<usize, BitLockerRuntimeError> {
        self.retain(|scope| scope.case_id != case_id || scope.data_source_id != data_source_id)
    }

    pub fn invalidate_case(&self, case_id: &str) -> Result<usize, BitLockerRuntimeError> {
        self.retain(|scope| scope.case_id != case_id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().map(|volumes| volumes.len()).unwrap_or_default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn retain(
        &self,
        keep: impl Fn(&BitLockerVolumeScope) -> bool,
    ) -> Result<usize, BitLockerRuntimeError> {
        let mut volumes = self.lock()?;
        let before = volumes.len();
        volumes.retain(|scope, _| keep(scope));
        Ok(before - volumes.len())
    }

    fn lock(
        &self,
    ) -> Result<
        std::sync::MutexGuard<'_, HashMap<BitLockerVolumeScope, Arc<UnlockedVolume>>>,
        BitLockerRuntimeError,
    > {
        self.volumes
            .lock()
            .map_err(|_| BitLockerRuntimeError::RegistryUnavailable)
    }
}
