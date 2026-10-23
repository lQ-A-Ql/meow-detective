use std::fmt;
use std::io;
use std::path::Path;
use std::sync::Arc;
#[cfg(windows)]
use std::time::Duration;

use tantivy::directory::error::{DeleteError, LockError, OpenReadError, OpenWriteError};
use tantivy::directory::{
    Directory, DirectoryLock, FileHandle, Lock, MmapDirectory, WatchCallback, WatchHandle, WritePtr,
};

#[cfg(windows)]
const RETRY_DELAYS_MS: &[u64] = &[10, 20, 40, 80, 160, 320, 640, 1000];

#[derive(Clone)]
pub(super) struct RetryMmapDirectory {
    inner: MmapDirectory,
}

impl RetryMmapDirectory {
    pub(super) fn open(path: &Path) -> tantivy::Result<Self> {
        Ok(Self {
            inner: MmapDirectory::open(path)?,
        })
    }
}

impl fmt::Debug for RetryMmapDirectory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(formatter)
    }
}

impl Directory for RetryMmapDirectory {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        retry(
            || self.inner.get_file_handle(path),
            retryable_open_read_error,
        )
    }

    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        retry(|| self.inner.delete(path), retryable_delete_error)
    }

    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        retry(|| self.inner.exists(path), retryable_open_read_error)
    }

    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        retry(|| self.inner.open_write(path), retryable_open_write_error)
    }

    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        retry(|| self.inner.atomic_read(path), retryable_open_read_error)
    }

    fn atomic_write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        retry(|| self.inner.atomic_write(path, data), retryable_io_error)
    }

    fn acquire_lock(&self, lock: &Lock) -> Result<DirectoryLock, LockError> {
        retry(|| self.inner.acquire_lock(lock), retryable_lock_error)
    }

    fn sync_directory(&self) -> io::Result<()> {
        retry(|| self.inner.sync_directory(), retryable_io_error)
    }

    fn watch(&self, watch_callback: WatchCallback) -> tantivy::Result<WatchHandle> {
        self.inner.watch(watch_callback)
    }
}

fn retry<T, E>(
    mut operation: impl FnMut() -> Result<T, E>,
    retryable: impl Fn(&E) -> bool,
) -> Result<T, E> {
    // Every operation passed here is safe to start again after a transient
    // Windows sharing failure. The final error is returned unchanged.
    #[cfg(windows)]
    for delay_ms in RETRY_DELAYS_MS {
        match operation() {
            Err(error) if retryable(&error) => {
                std::thread::sleep(Duration::from_millis(*delay_ms));
            }
            result => return result,
        }
    }
    operation()
}

fn retryable_io_error(error: &io::Error) -> bool {
    let transient_kind = matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::PermissionDenied | io::ErrorKind::WouldBlock
    );
    #[cfg(windows)]
    // Windows sharing and lock violations can be reported as Uncategorized.
    let sharing_violation = matches!(error.raw_os_error(), Some(5 | 32 | 33));
    #[cfg(not(windows))]
    let sharing_violation = false;
    transient_kind || sharing_violation
}

fn retryable_open_write_error(error: &OpenWriteError) -> bool {
    match error {
        // create_new can report an existing scanner-held candidate while the
        // other handle is in the middle of releasing the path.
        OpenWriteError::FileAlreadyExists(_) => true,
        OpenWriteError::IoError { io_error, .. } => retryable_io_error(io_error),
    }
}

fn retryable_delete_error(error: &DeleteError) -> bool {
    matches!(
        error,
        DeleteError::IoError { io_error, .. }
            if retryable_io_error(io_error)
    )
}

fn retryable_open_read_error(error: &OpenReadError) -> bool {
    matches!(
        error,
        OpenReadError::IoError { io_error, .. }
            if retryable_io_error(io_error)
    )
}

fn retryable_lock_error(error: &LockError) -> bool {
    matches!(
        error,
        LockError::IoError(io_error)
            if retryable_io_error(io_error)
    )
}

#[cfg(test)]
#[path = "../../tests/unit/indexer/retry_directory.rs"]
mod tests;
