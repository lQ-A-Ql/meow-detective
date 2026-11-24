use super::*;
use std::{sync::atomic::AtomicUsize, time::Instant};

mod dispatch;
mod input;
mod metrics;
pub use input::NativeWriteInput;
pub(super) use metrics::NativeWriteMetrics;
pub use metrics::NativeWriteReport;

#[derive(Debug, Clone, Copy)]
pub struct WriteSessionConfig {
    pub workers: usize,
    pub batch_bytes: usize,
}

impl Default for WriteSessionConfig {
    fn default() -> Self {
        Self {
            workers: 8,
            batch_bytes: 8 * 1024 * 1024,
        }
    }
}

impl WriteSessionConfig {
    pub(crate) fn validate(self) -> Result<()> {
        if !(4..=16).contains(&self.workers)
            || self.batch_bytes == 0
            || self.batch_bytes.saturating_mul(self.workers * 2) > 256 * 1024 * 1024
        {
            return Err(FjallStorageError::WriteSessionLimit);
        }
        Ok(())
    }
}

pub struct SourceWriteSession {
    pub(super) database: SourceDatabase,
    pub(super) config: WriteSessionConfig,
    pub(super) metrics: NativeWriteMetrics,
    started: Instant,
    completed: bool,
    dispatch_active: AtomicBool,
}

impl SourceDatabase {
    pub fn begin_write_session(&self, config: WriteSessionConfig) -> Result<SourceWriteSession> {
        config.validate()?;
        let _permit = self
            .inner
            .lifecycle
            .write()
            .map_err(|_| FjallStorageError::LockPoisoned)?;
        self.ensure_regular_write()?;
        let marker = self.path().join(NATIVE_WRITE_INCOMPLETE_FILE);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker)?;
        file.sync_all()?;
        self.inner.session_active.store(true, Ordering::Release);
        Ok(SourceWriteSession {
            database: self.clone(),
            config,
            metrics: NativeWriteMetrics::default(),
            started: Instant::now(),
            completed: false,
            dispatch_active: AtomicBool::new(false),
        })
    }
}

impl SourceWriteSession {
    pub fn run<T: NativeWriteInput>(
        &self,
        cancel: &AtomicBool,
        next_batch: impl FnMut() -> Result<Option<Vec<T>>>,
        route: impl Fn(&T) -> u64,
        encode: impl Fn(&SourceReadHandle, Vec<T>) -> Result<Vec<WriteAction>> + Sync,
    ) -> Result<()> {
        self.dispatch_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| FjallStorageError::WriteSessionBusy)?;
        let _dispatch = DispatchPermit(&self.dispatch_active);
        dispatch::run(self, cancel, next_batch, route, encode)
    }

    pub fn finish(mut self) -> Result<NativeWriteReport> {
        let _permit = self
            .database
            .inner
            .lifecycle
            .write()
            .map_err(|_| FjallStorageError::LockPoisoned)?;
        if self.database.inner.session_failed.load(Ordering::Acquire) {
            return Err(FjallStorageError::WriteSessionFailed);
        }
        self.database.persist(PersistMode::SyncAll)?;
        std::fs::remove_file(self.database.path().join(NATIVE_WRITE_INCOMPLETE_FILE))?;
        let report = self
            .metrics
            .report(self.config.workers, self.started.elapsed());
        self.database
            .inner
            .session_active
            .store(false, Ordering::Release);
        self.completed = true;
        Ok(report)
    }

    pub(super) fn ensure_active(&self, cancel: &AtomicBool) -> Result<()> {
        if cancel.load(Ordering::Acquire) {
            return Err(FjallStorageError::Cancelled);
        }
        if self.database.inner.session_failed.load(Ordering::Acquire) {
            return Err(FjallStorageError::WriteSessionFailed);
        }
        Ok(())
    }

    pub(super) fn fail(&self) {
        self.database
            .inner
            .session_failed
            .store(true, Ordering::Release);
    }
}

struct DispatchPermit<'a>(&'a AtomicBool);

impl Drop for DispatchPermit<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Drop for SourceWriteSession {
    fn drop(&mut self) {
        if !self.completed {
            self.fail();
            self.database
                .inner
                .session_active
                .store(false, Ordering::Release);
        }
    }
}
