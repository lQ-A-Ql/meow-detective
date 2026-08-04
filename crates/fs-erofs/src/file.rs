use std::io::{self, Read, Seek, SeekFrom};

use crate::io::{block_offset, SharedReader};
use crate::{ErofsError, Result};

pub(crate) struct ErofsFile {
    source: SharedReader,
    volume_offset: u64,
    block_size: usize,
    block_count: u64,
    start_block: u64,
    size: u64,
    cursor: u64,
}

impl ErofsFile {
    pub(crate) fn new(
        source: SharedReader,
        volume_offset: u64,
        block_size: usize,
        block_count: u64,
        start_block: u64,
        size: u64,
    ) -> Result<Self> {
        let required = size.div_ceil(block_size as u64);
        if required != 0
            && (start_block >= block_count
                || start_block
                    .checked_add(required)
                    .is_none_or(|end| end > block_count))
        {
            return Err(ErofsError::Invalid(format!(
                "file blocks {start_block}.. exceed filesystem"
            )));
        }
        Ok(Self {
            source,
            volume_offset,
            block_size,
            block_count,
            start_block,
            size,
            cursor: 0,
        })
    }

    fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize> {
        if offset >= self.size || output.is_empty() {
            return Ok(0);
        }
        let requested = output
            .len()
            .min(usize::try_from(self.size - offset).unwrap_or(usize::MAX));
        let block = self.start_block + offset / self.block_size as u64;
        if block >= self.block_count {
            return Err(ErofsError::Invalid(
                "file block is outside filesystem".to_string(),
            ));
        }
        let physical = block_offset(self.volume_offset, block, self.block_size)?
            .checked_add(offset % self.block_size as u64)
            .ok_or_else(|| ErofsError::Invalid("file read offset overflows".to_string()))?;
        let mut source = self
            .source
            .lock()
            .map_err(|_| ErofsError::Invalid("evidence reader lock is poisoned".to_string()))?;
        source.seek(SeekFrom::Start(physical))?;
        source.read_exact(&mut output[..requested])?;
        Ok(requested)
    }
}

impl Read for ErofsFile {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let read = self
            .read_at(self.cursor, output)
            .map_err(ErofsError::into_io)?;
        self.cursor = self.cursor.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for ErofsFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.size) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.cursor) + i128::from(value),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "EROFS file seek is outside the addressable range",
            ));
        }
        self.cursor = next as u64;
        Ok(self.cursor)
    }
}
