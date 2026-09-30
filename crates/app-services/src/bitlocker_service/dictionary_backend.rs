use super::{
    dictionary_gpu::GpuWorkers,
    dictionary_parallel::{worker_count, BatchResult, DictionaryWorkers},
    BitLockerServiceError,
};
use std::sync::atomic::AtomicBool;
use transport::dto::BitLockerDictionaryBackendDto;
use volume_bitlocker::VolumeIdentity;
use zeroize::Zeroizing;

pub(super) enum DictionaryBackend {
    Cpu(DictionaryWorkers),
    Gpu(Box<GpuWorkers>),
}
impl DictionaryBackend {
    pub(super) fn new(
        mode: BitLockerDictionaryBackendDto,
        identities: &[VolumeIdentity],
    ) -> Result<Self, BitLockerServiceError> {
        match mode {
            BitLockerDictionaryBackendDto::Cpu => Ok(Self::Cpu(DictionaryWorkers::new()?)),
            BitLockerDictionaryBackendDto::Gpu => {
                Ok(Self::Gpu(Box::new(GpuWorkers::new(identities)?)))
            }
        }
    }
    pub(super) fn batch_size(&self) -> usize {
        match self {
            Self::Cpu(_) => worker_count(usize::MAX).saturating_mul(4).max(1),
            Self::Gpu(_) => 2048,
        }
    }
    pub(super) fn try_batch(
        &self,
        identities: &[VolumeIdentity],
        candidates: Vec<Zeroizing<String>>,
        cancel: &AtomicBool,
    ) -> Result<BatchResult, BitLockerServiceError> {
        match self {
            Self::Cpu(workers) => Ok(workers.try_batch(identities, candidates, cancel)),
            Self::Gpu(workers) => workers
                .try_batch(identities, candidates, cancel)
                .map_err(Into::into),
        }
    }
}
