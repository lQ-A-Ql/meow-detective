//! Opaque RAII resources, supplied by the application without depending on it.
use std::{io, sync::Arc};
pub trait ReaderResourceHold: Send + Sync {}
impl<T: Send + Sync> ReaderResourceHold for T {}
pub trait ReaderAdmission: Send + Sync {
    fn acquire(&self) -> io::Result<Arc<dyn ReaderResourceHold>>;
    fn reserve_work(&self, _bytes: u64) -> io::Result<Option<Arc<dyn ReaderResourceHold>>> {
        Ok(None)
    }
}
pub trait ReaderAllocationScope: Send + Sync {
    fn active(&self) -> bool;
    fn acquire(&self, bytes: u64, handles: u64) -> io::Result<Arc<dyn ReaderResourceHold>>;
    fn acquire_work(&self, bytes: u64, workers: u64) -> io::Result<Arc<dyn ReaderResourceHold>>;
}
