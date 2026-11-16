//! Bounded random-access adapters used by the image structure walkers.

use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::file_service::FileServiceError;

use super::{ImageAnalyzeOptions, ImageByteSource};

const READ_CHUNK: usize = 256 * 1024;

#[derive(Debug)]
pub(super) enum ScanError {
    Read(FileServiceError),
    Limit,
    Corrupt,
    Cancelled,
}

pub(super) struct ScanReader<'a> {
    source: &'a mut dyn ImageByteSource,
    cancel: &'a AtomicBool,
    size: u64,
    read_remaining: usize,
    scan_remaining: usize,
    pub bytes_read: u64,
}

impl<'a> ScanReader<'a> {
    pub(super) fn new(
        source: &'a mut dyn ImageByteSource,
        cancel: &'a AtomicBool,
        options: &ImageAnalyzeOptions,
    ) -> Self {
        let size = source.size();
        Self {
            source,
            cancel,
            size,
            read_remaining: options.random_read_limit,
            scan_remaining: options.scan_limit,
            bytes_read: 0,
        }
    }

    pub(super) fn size(&self) -> u64 {
        self.size
    }

    pub(super) fn read(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, ScanError> {
        if length == 0 {
            return Ok(Vec::new());
        }
        self.check_cancel()?;
        if length > self.read_remaining {
            return Err(ScanError::Limit);
        }
        let end = offset
            .checked_add(length as u64)
            .ok_or(ScanError::Corrupt)?;
        if end > self.size {
            return Err(ScanError::Corrupt);
        }
        self.read_remaining -= length;
        let bytes = self.read_exact(offset, length)?;
        Ok(bytes)
    }

    pub(super) fn read_unbudgeted(
        &mut self,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, ScanError> {
        if length == 0 {
            return Ok(Vec::new());
        }
        let end = offset
            .checked_add(length as u64)
            .ok_or(ScanError::Corrupt)?;
        if end > self.size {
            return Err(ScanError::Corrupt);
        }
        self.read_exact(offset, length)
    }

    pub(super) fn read_scan(
        &mut self,
        offset: u64,
        max_length: usize,
    ) -> Result<Option<Vec<u8>>, ScanError> {
        if self.scan_remaining == 0 || offset >= self.size {
            return Ok(None);
        }
        let available = max_length
            .min(self.scan_remaining)
            .min((self.size - offset) as usize);
        if available == 0 {
            return Ok(None);
        }
        self.scan_remaining -= available;
        self.read_exact(offset, available).map(Some)
    }

    fn read_exact(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, ScanError> {
        let mut bytes = Vec::with_capacity(length);
        while bytes.len() < length {
            self.check_cancel()?;
            let chunk_length = (length - bytes.len()).min(READ_CHUNK);
            let chunk = self
                .source
                .read_at(offset + bytes.len() as u64, chunk_length)
                .map_err(ScanError::Read)?;
            if chunk.len() != chunk_length {
                return Err(ScanError::Corrupt);
            }
            self.bytes_read += chunk.len() as u64;
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    fn check_cancel(&self) -> Result<(), ScanError> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(ScanError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Adapter for a seekable evidence reader (candidate extraction path).
pub struct SeekableSource<R> {
    reader: R,
    size: u64,
}

impl<R: Read + Seek> SeekableSource<R> {
    pub fn new(mut reader: R) -> Result<Self, FileServiceError> {
        let size = reader.seek(SeekFrom::End(0))?;
        Ok(Self { reader, size })
    }
}

impl<R: Read + Seek> ImageByteSource for SeekableSource<R> {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, FileServiceError> {
        if offset >= self.size {
            return Ok(Vec::new());
        }
        self.reader.seek(SeekFrom::Start(offset))?;
        let available = (self.size - offset).min(length as u64) as usize;
        let mut bytes = vec![0u8; available];
        self.reader.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}
