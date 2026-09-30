use std::sync::atomic::{AtomicBool, Ordering};
use volume_bitlocker::{
    unlock_volume_with_password_for_identities, BitLockerError, Passphrase,
    PasswordAccelerationTarget, VolumeIdentity,
};
use zeroize::Zeroizing;

use super::super::dictionary_parallel::BatchResult;
use super::{runtime, GpuError};

pub(crate) struct GpuWorkers {
    runtime: runtime::GpuRuntime,
    target: PasswordAccelerationTarget,
}

impl GpuWorkers {
    pub(crate) fn new(
        identities: &[VolumeIdentity],
    ) -> Result<Self, super::super::BitLockerServiceError> {
        let target = PasswordAccelerationTarget::new(identities)?;
        let runtime = runtime::GpuRuntime::new()?;
        // Validate driver/compiler output before admitting real candidates.
        let hashes = PasswordAccelerationTarget::initial_hashes(&[Passphrase::new(
            "OpenCL self-test".into(),
        )]);
        runtime.self_test(&hashes[0])?;
        Ok(Self { runtime, target })
    }

    pub(crate) fn try_batch(
        &self,
        identities: &[VolumeIdentity],
        candidates: Vec<Zeroizing<String>>,
        cancel: &AtomicBool,
    ) -> Result<BatchResult, GpuError> {
        let passwords: Vec<_> = candidates
            .into_iter()
            .map(|mut candidate| Passphrase::new(std::mem::take(&mut *candidate)))
            .collect();
        let hashes = PasswordAccelerationTarget::initial_hashes(&passwords);
        let mut all_keys = Vec::new();
        for salt in self.target.salts() {
            let Some(keys) = self.runtime.stretch(&hashes, salt, cancel)? else {
                return Ok(BatchResult {
                    verified: Ok(None),
                    attempted: 0,
                });
            };
            all_keys.push(keys);
        }
        if cancel.load(Ordering::Acquire) {
            return Ok(BatchResult {
                verified: Ok(None),
                attempted: 0,
            });
        }
        for (index, password) in passwords.iter().enumerate() {
            let keys = Zeroizing::new(all_keys.iter().map(|keys| keys[index]).collect::<Vec<_>>());
            match self.target.verify(&keys) {
                Ok(_) => {
                    let verified =
                        unlock_volume_with_password_for_identities(identities, password).map(Some);
                    return Ok(BatchResult {
                        verified,
                        attempted: index.saturating_add(1),
                    });
                }
                Err(BitLockerError::CredentialRejected) => {}
                Err(error) => {
                    return Ok(BatchResult {
                        verified: Err(error),
                        attempted: passwords.len(),
                    })
                }
            }
        }
        Ok(BatchResult {
            verified: Ok(None),
            attempted: passwords.len(),
        })
    }
}
