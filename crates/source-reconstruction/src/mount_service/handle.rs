use std::io;
use std::sync::{Arc, Mutex};

use evidence_core::FileSystemReader;
use evidence_mount::MountFileHandle;

pub(crate) type SharedFilesystem = Arc<Mutex<Box<dyn FileSystemReader + Send>>>;

const READ_AHEAD_BYTES: usize = 256 * 1024;

struct ReadWindow {
    offset: u64,
    data: Vec<u8>,
}

pub(crate) struct FilesystemRangeHandle {
    filesystem: SharedFilesystem,
    paths: Vec<String>,
    size: u64,
    resolved_path_index: Option<usize>,
    read_window: Option<ReadWindow>,
    last_request_end: Option<u64>,
}

impl FilesystemRangeHandle {
    pub(crate) fn new(filesystem: SharedFilesystem, paths: Vec<String>, size: u64) -> Self {
        Self {
            filesystem,
            paths,
            size,
            resolved_path_index: None,
            read_window: None,
            last_request_end: None,
        }
    }

    fn cached_range(&self, offset: u64, length: usize) -> Option<Vec<u8>> {
        let window = self.read_window.as_ref()?;
        let relative = offset.checked_sub(window.offset)?;
        let start = usize::try_from(relative).ok()?;
        let end = start.checked_add(length)?;
        window.data.get(start..end).map(<[u8]>::to_vec)
    }

    fn fetch_length(&self, offset: u64, length: usize) -> usize {
        let remaining = self.size.saturating_sub(offset);
        let requested = if self.last_request_end == Some(offset) {
            length.max(READ_AHEAD_BYTES)
        } else {
            length
        };
        usize::try_from(remaining.min(requested as u64)).unwrap_or(requested)
    }

    fn read_from_filesystem(&mut self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let filesystem = Arc::clone(&self.filesystem);
        let filesystem = filesystem
            .lock()
            .map_err(|_| io::Error::other("filesystem reader lock is poisoned"))?;
        let mut last_error = None;

        if let Some(index) = self.resolved_path_index {
            match filesystem.read_file_range(&self.paths[index], offset, length) {
                Ok(bytes) => return Ok(bytes),
                Err(error) => last_error = Some(error),
            }
        }

        for (index, path) in self.paths.iter().enumerate() {
            if Some(index) == self.resolved_path_index {
                continue;
            }
            match filesystem.read_file_range(path, offset, length) {
                Ok(bytes) => {
                    self.resolved_path_index = Some(index);
                    return Ok(bytes);
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "no filesystem path candidate")
        }))
    }
}

impl MountFileHandle for FilesystemRangeHandle {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        if offset > self.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file offset is outside the mounted file",
            ));
        }
        let bounded_length = usize::try_from(self.size.saturating_sub(offset))
            .unwrap_or(usize::MAX)
            .min(length);
        if bounded_length == 0 {
            return Ok(Vec::new());
        }

        if let Some(data) = self.cached_range(offset, bounded_length) {
            self.last_request_end = Some(offset.saturating_add(data.len() as u64));
            return Ok(data);
        }

        let fetch_length = self.fetch_length(offset, bounded_length);
        let fetched = self.read_from_filesystem(offset, fetch_length)?;
        let result_length = bounded_length.min(fetched.len());
        let result = fetched[..result_length].to_vec();
        self.read_window = Some(ReadWindow {
            offset,
            data: fetched,
        });
        self.last_request_end = Some(offset.saturating_add(result_length as u64));
        Ok(result)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/handle.rs"]
mod tests;
