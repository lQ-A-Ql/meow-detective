use std::io::{self, Read, Seek, SeekFrom};

use crate::io::{block_offset, SharedReader};
use crate::{F2fsError, Result, F2FS_BLOCK_SIZE};

const NULL_ADDRESS: u32 = 0;
const NEW_ADDRESS: u32 = u32::MAX;
const COMPRESSED_ADDRESS: u32 = u32::MAX - 1;

pub(crate) struct F2fsFile {
    source: SharedReader,
    volume_offset: u64,
    main_block: u32,
    block_count: u64,
    size: u64,
    blocks: Vec<u32>,
    cursor: u64,
}

impl F2fsFile {
    pub(crate) fn new(
        source: SharedReader,
        volume_offset: u64,
        main_block: u32,
        block_count: u64,
        size: u64,
        blocks: Vec<u32>,
    ) -> Result<Self> {
        validate_blocks(&blocks, main_block, block_count)?;
        Ok(Self {
            source,
            volume_offset,
            main_block,
            block_count,
            size,
            blocks,
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
            let block_index = (position / F2FS_BLOCK_SIZE as u64) as usize;
            let within = (position % F2FS_BLOCK_SIZE as u64) as usize;
            let length = (F2FS_BLOCK_SIZE - within).min(requested - written);
            let address = *self
                .blocks
                .get(block_index)
                .ok_or_else(|| F2fsError::Unsupported("indirect file block lookup".to_string()))?;
            if address == NULL_ADDRESS {
                output[written..written + length].fill(0);
            } else {
                self.read_physical(address, within, &mut output[written..written + length])?;
            }
            written += length;
        }
        Ok(written)
    }

    fn read_physical(&self, block: u32, within: usize, output: &mut [u8]) -> Result<()> {
        if block < self.main_block || u64::from(block) >= self.block_count {
            return Err(F2fsError::Invalid(format!(
                "file data block {block} is outside the main area"
            )));
        }
        let offset = block_offset(self.volume_offset, block)?
            .checked_add(within as u64)
            .ok_or_else(|| F2fsError::Invalid("file read offset overflows".to_string()))?;
        let mut source = self
            .source
            .lock()
            .map_err(|_| F2fsError::Invalid("evidence reader lock is poisoned".to_string()))?;
        source.seek(SeekFrom::Start(offset))?;
        source.read_exact(output)?;
        Ok(())
    }
}

impl Read for F2fsFile {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let read = self
            .read_at(self.cursor, output)
            .map_err(F2fsError::into_io)?;
        self.cursor = self.cursor.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for F2fsFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.size) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.cursor) + i128::from(value),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "F2FS file seek is outside the addressable range",
            ));
        }
        self.cursor = next as u64;
        Ok(self.cursor)
    }
}

fn validate_blocks(blocks: &[u32], main_block: u32, block_count: u64) -> Result<()> {
    for block in blocks {
        match *block {
            NULL_ADDRESS => {}
            NEW_ADDRESS => {
                return Err(F2fsError::Invalid(
                    "file references an unallocated NEW_ADDR block".to_string(),
                ));
            }
            COMPRESSED_ADDRESS => {
                return Err(F2fsError::Unsupported(
                    "compressed F2FS clusters".to_string(),
                ));
            }
            value if value < main_block || u64::from(value) >= block_count => {
                return Err(F2fsError::Invalid(format!(
                    "file data block {value} is outside the main area"
                )));
            }
            _ => {}
        }
    }
    Ok(())
}
