use std::io::{self, Read, Seek, SeekFrom};

use crate::io::{block_offset, SharedReader};
use crate::{ErofsError, Result};

pub(crate) struct ErofsFile {
    source: SharedReader,
    volume_offset: u64,
    block_size: usize,
    block_count: u64,
    start_block: u64,
    external_size: u64,
    inline_offset: Option<u64>,
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
        inline_offset: Option<u64>,
        size: u64,
    ) -> Result<Self> {
        let external_size = if inline_offset.is_some() && size != 0 {
            (size - 1) / block_size as u64 * block_size as u64
        } else {
            size
        };
        validate_external_blocks(start_block, external_size, block_size, block_count)?;
        validate_inline_tail(
            inline_offset,
            size - external_size,
            volume_offset,
            block_size,
        )?;
        Ok(Self {
            source,
            volume_offset,
            block_size,
            block_count,
            start_block,
            external_size,
            inline_offset,
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
        let mut written = 0usize;
        while written < requested {
            let position = offset + written as u64;
            let (physical, available) = self.physical_range(position)?;
            let length = available.min((requested - written) as u64) as usize;
            let mut source = self
                .source
                .lock()
                .map_err(|_| ErofsError::Invalid("evidence reader lock is poisoned".to_string()))?;
            source.seek(SeekFrom::Start(physical))?;
            source.read_exact(&mut output[written..written + length])?;
            written += length;
        }
        Ok(requested)
    }

    fn physical_range(&self, position: u64) -> Result<(u64, u64)> {
        if position < self.external_size {
            let block = self.start_block + position / self.block_size as u64;
            if block >= self.block_count {
                return Err(ErofsError::Invalid(
                    "file block is outside filesystem".to_string(),
                ));
            }
            let physical = block_offset(self.volume_offset, block, self.block_size)?
                .checked_add(position % self.block_size as u64)
                .ok_or_else(|| ErofsError::Invalid("file read offset overflows".to_string()))?;
            return Ok((physical, self.external_size - position));
        }
        let inline = self.inline_offset.ok_or_else(|| {
            ErofsError::Invalid("file position has no physical mapping".to_string())
        })?;
        let relative = position - self.external_size;
        let physical = inline
            .checked_add(relative)
            .ok_or_else(|| ErofsError::Invalid("inline read offset overflows".to_string()))?;
        Ok((physical, self.size - position))
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

fn validate_external_blocks(
    start_block: u64,
    external_size: u64,
    block_size: usize,
    block_count: u64,
) -> Result<()> {
    let required = external_size.div_ceil(block_size as u64);
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
    Ok(())
}

fn validate_inline_tail(
    inline_offset: Option<u64>,
    inline_length: u64,
    volume_offset: u64,
    block_size: usize,
) -> Result<()> {
    let Some(offset) = inline_offset else {
        return Ok(());
    };
    let within = offset
        .checked_sub(volume_offset)
        .ok_or_else(|| ErofsError::Invalid("inline offset precedes filesystem".to_string()))?
        % block_size as u64;
    if within
        .checked_add(inline_length)
        .is_none_or(|end| end > block_size as u64)
    {
        return Err(ErofsError::Invalid(
            "inline file tail exceeds its metadata block".to_string(),
        ));
    }
    Ok(())
}
