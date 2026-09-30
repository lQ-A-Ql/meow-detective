use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use volume_bitlocker::{
    unlock_volume_with_password_for_identities, BitLockerError, Passphrase, VerifiedUnlock,
    VolumeIdentity,
};
use zeroize::Zeroizing;

use super::error::BitLockerServiceError;

// Share CPU workers across attacks so parallel jobs cannot each create a full
// machine's worth of threads. The mutex is held only during initialization.
static DICTIONARY_POOL: Mutex<Option<Arc<rayon::ThreadPool>>> = Mutex::new(None);

pub(super) fn worker_count(candidate_count: usize) -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(candidate_count.max(1))
}

pub(super) struct DictionaryWorkers {
    pool: Arc<rayon::ThreadPool>,
}

pub(super) struct BatchResult {
    pub(super) verified: Result<Option<VerifiedUnlock>, BitLockerError>,
    pub(super) attempted: usize,
}

impl DictionaryWorkers {
    pub(super) fn new() -> Result<Self, BitLockerServiceError> {
        let mut shared = DICTIONARY_POOL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(pool) = shared.as_ref() {
            return Ok(Self {
                pool: Arc::clone(pool),
            });
        }
        let pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(worker_count(usize::MAX))
                .thread_name(|index| format!("bitlocker-kdf-{index}"))
                .build()
                .map_err(BitLockerServiceError::DictionaryWorkers)?,
        );
        *shared = Some(Arc::clone(&pool));
        Ok(Self { pool })
    }

    pub(super) fn try_batch(
        &self,
        identities: &[VolumeIdentity],
        candidates: Vec<Zeroizing<String>>,
        cancel: &AtomicBool,
    ) -> BatchResult {
        let stop = Arc::new(AtomicBool::new(false));
        let results = self.pool.install(|| {
            candidates
                .into_par_iter()
                .map(|mut candidate| {
                    if cancel.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
                        return (Ok(None), false);
                    }
                    let passphrase = Passphrase::new(std::mem::take(&mut *candidate));
                    match unlock_volume_with_password_for_identities(identities, &passphrase) {
                        Ok(verified) => {
                            stop.store(true, Ordering::Release);
                            (Ok(Some(verified)), true)
                        }
                        Err(BitLockerError::CredentialRejected) => (Ok(None), true),
                        Err(error) => {
                            stop.store(true, Ordering::Release);
                            (Err(error), true)
                        }
                    }
                })
                .collect::<Vec<_>>()
        });
        let attempted = results.iter().filter(|(_, attempted)| *attempted).count();
        let verified = results
            .into_iter()
            .map(|(verified, _)| verified)
            .collect::<Result<Vec<_>, _>>()
            .map(|values| values.into_iter().flatten().next());
        BatchResult {
            verified,
            attempted,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_dictionary_parallel.rs"]
mod tests;
