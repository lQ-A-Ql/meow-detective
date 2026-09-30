use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rayon::prelude::*;
use volume_bitlocker::{
    unlock_volume_with_password_for_identities, BitLockerError, Passphrase, VerifiedUnlock,
    VolumeIdentity,
};
use zeroize::Zeroizing;

const MAX_DICTIONARY_WORKERS: usize = 64;

pub(super) fn worker_count(candidate_count: usize) -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(MAX_DICTIONARY_WORKERS)
        .min(candidate_count.max(1))
}

pub(super) struct DictionaryWorkers {
    pool: rayon::ThreadPool,
}

impl DictionaryWorkers {
    pub(super) fn new(candidate_count: usize) -> Result<Self, BitLockerError> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count(candidate_count))
            .thread_name(|index| format!("bitlocker-kdf-{index}"))
            .build()
            .map_err(|error| BitLockerError::MetadataUnreadable {
                reason: format!("could not start dictionary workers: {error}"),
            })?;
        Ok(Self { pool })
    }

    pub(super) fn try_batch(
        &self,
        identities: &[VolumeIdentity],
        candidates: Vec<Zeroizing<String>>,
        cancel: &AtomicBool,
    ) -> Result<Option<VerifiedUnlock>, BitLockerError> {
        let stop = Arc::new(AtomicBool::new(false));
        let results = self.pool.install(|| {
            candidates
                .into_par_iter()
                .map(|mut candidate| {
                    if cancel.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
                        return Ok(None);
                    }
                    let passphrase = Passphrase::new(std::mem::take(&mut *candidate));
                    match unlock_volume_with_password_for_identities(identities, &passphrase) {
                        Ok(verified) => {
                            stop.store(true, Ordering::Release);
                            Ok(Some(verified))
                        }
                        Err(BitLockerError::CredentialRejected) => Ok(None),
                        Err(error) => Err(error),
                    }
                })
                .collect::<Result<Vec<_>, _>>()
        })?;
        Ok(results.into_iter().flatten().next())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_dictionary_parallel.rs"]
mod tests;
