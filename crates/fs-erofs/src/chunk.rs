use std::io::{Read, Seek, SeekFrom};

use crate::inode::ErofsInode;
use crate::io::{block_offset, read_exact_at, read_u16, read_u32, SharedReader};
use crate::{ErofsError, ErofsSuperblock, Result};

const CHUNK_FORMAT_BLOCK_BITS_MASK: u16 = 0x001f;
const CHUNK_FORMAT_INDEXES: u16 = 0x0020;
const CHUNK_FORMAT_48BIT: u16 = 0x0040;
const CHUNK_FORMAT_ALL: u16 = 0x007f;
const NULL_ADDRESS_32: u64 = u32::MAX as u64;
const NULL_ADDRESS_48: u64 = (1u64 << 48) - 1;

pub(crate) struct ErofsChunkFile {
    source: SharedReader,
    volume_offset: u64,
    block_size: usize,
    block_count: u64,
    index_offset: u64,
    layout: ChunkLayout,
    size: u64,
}

#[derive(Clone, Copy)]
struct ChunkLayout {
    chunk_size: u64,
    entry_size: u64,
    indexed: bool,
    address_mask: u64,
}

impl ErofsChunkFile {
    pub(crate) fn new(
        source: SharedReader,
        volume_offset: u64,
        superblock: &ErofsSuperblock,
        inode: &ErofsInode,
    ) -> Result<Self> {
        let format = inode.chunk_format.ok_or_else(|| {
            ErofsError::Invalid(format!("inode {} has no chunk format", inode.nid))
        })?;
        let layout = ChunkLayout::parse(format, superblock.block_size)?;
        let metadata_end = inode
            .source_offset
            .checked_add(inode.inode_size as u64)
            .and_then(|offset| offset.checked_add(inode.xattr_size as u64))
            .ok_or_else(|| ErofsError::Invalid("chunk index offset overflows".to_string()))?;
        let index_offset = align_up(metadata_end, layout.entry_size)?;
        validate_index_table(index_offset, inode.size, layout)?;
        Ok(Self {
            source,
            volume_offset,
            block_size: superblock.block_size,
            block_count: superblock.block_count,
            index_offset,
            layout,
            size: inode.size,
        })
    }

    pub(crate) fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize> {
        if offset >= self.size || output.is_empty() {
            return Ok(0);
        }
        let requested = output
            .len()
            .min(usize::try_from(self.size - offset).unwrap_or(usize::MAX));
        let mut written = 0usize;
        while written < requested {
            let position = offset + written as u64;
            let chunk_index = position / self.layout.chunk_size;
            let within = position % self.layout.chunk_size;
            let length = usize::try_from(
                (self.layout.chunk_size - within).min((requested - written) as u64),
            )
            .map_err(|_| ErofsError::Invalid("chunk read length exceeds usize".to_string()))?;
            match self.read_address(chunk_index)? {
                Some(block) => {
                    self.read_physical(block, within, &mut output[written..written + length])?
                }
                None => output[written..written + length].fill(0),
            }
            written += length;
        }
        Ok(written)
    }

    fn read_address(&self, chunk_index: u64) -> Result<Option<u64>> {
        let offset = chunk_index
            .checked_mul(self.layout.entry_size)
            .and_then(|relative| self.index_offset.checked_add(relative))
            .ok_or_else(|| ErofsError::Invalid("chunk index address overflows".to_string()))?;
        let bytes = read_exact_at(&self.source, offset, self.layout.entry_size as usize)?;
        if !self.layout.indexed {
            let address = u64::from(read_u32(&bytes, 0, "chunk block address")?);
            return Ok((address != NULL_ADDRESS_32).then_some(address));
        }

        let high = u64::from(read_u16(&bytes, 0, "chunk block high bits")?);
        let device = read_u16(&bytes, 2, "chunk device id")?;
        let low = u64::from(read_u32(&bytes, 4, "chunk block low bits")?);
        let address = ((high << 32) | low) & self.layout.address_mask;
        if address == self.layout.address_mask {
            return Ok(None);
        }
        if device != 0 {
            return Err(ErofsError::Unsupported(format!(
                "chunk mapping on external device {device}"
            )));
        }
        Ok(Some(address))
    }

    fn read_physical(&self, block: u64, within: u64, output: &mut [u8]) -> Result<()> {
        let filesystem_size = self
            .block_count
            .checked_mul(self.block_size as u64)
            .ok_or_else(|| ErofsError::Invalid("filesystem size overflows".to_string()))?;
        let relative = block
            .checked_mul(self.block_size as u64)
            .and_then(|offset| offset.checked_add(within))
            .ok_or_else(|| ErofsError::Invalid("chunk data offset overflows".to_string()))?;
        if relative
            .checked_add(output.len() as u64)
            .is_none_or(|end| end > filesystem_size)
        {
            return Err(ErofsError::Invalid(format!(
                "chunk data block {block} exceeds filesystem"
            )));
        }
        let physical = block_offset(self.volume_offset, block, self.block_size)?
            .checked_add(within)
            .ok_or_else(|| ErofsError::Invalid("chunk read offset overflows".to_string()))?;
        let mut source = self
            .source
            .lock()
            .map_err(|_| ErofsError::Invalid("evidence reader lock is poisoned".to_string()))?;
        source.seek(SeekFrom::Start(physical))?;
        source.read_exact(output)?;
        Ok(())
    }
}

impl ChunkLayout {
    fn parse(format: u16, block_size: usize) -> Result<Self> {
        if format & !CHUNK_FORMAT_ALL != 0 {
            return Err(ErofsError::Unsupported(format!(
                "chunk format flags {:#x}",
                format & !CHUNK_FORMAT_ALL
            )));
        }
        let indexed = format & CHUNK_FORMAT_INDEXES != 0;
        let wide = format & CHUNK_FORMAT_48BIT != 0;
        if wide && !indexed {
            return Err(ErofsError::Invalid(
                "48-bit chunk addresses require indexed entries".to_string(),
            ));
        }
        let extra_bits = u32::from(format & CHUNK_FORMAT_BLOCK_BITS_MASK);
        let chunk_size = (block_size as u64)
            .checked_shl(extra_bits)
            .ok_or_else(|| ErofsError::Unsupported("chunk size exceeds u64".to_string()))?;
        Ok(Self {
            chunk_size,
            entry_size: if indexed { 8 } else { 4 },
            indexed,
            address_mask: if wide {
                NULL_ADDRESS_48
            } else {
                NULL_ADDRESS_32
            },
        })
    }
}

fn align_up(value: u64, alignment: u64) -> Result<u64> {
    value
        .checked_add(alignment - 1)
        .map(|sum| sum / alignment * alignment)
        .ok_or_else(|| ErofsError::Invalid("chunk index alignment overflows".to_string()))
}

fn validate_index_table(index_offset: u64, size: u64, layout: ChunkLayout) -> Result<()> {
    let entries = size.div_ceil(layout.chunk_size);
    entries
        .checked_mul(layout.entry_size)
        .and_then(|bytes| index_offset.checked_add(bytes))
        .ok_or_else(|| ErofsError::Invalid("chunk index table overflows".to_string()))?;
    Ok(())
}
