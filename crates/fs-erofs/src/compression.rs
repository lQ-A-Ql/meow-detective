mod index;
mod map;

use std::sync::{Arc, Mutex};

use crate::inode::ErofsInode;
use crate::io::{block_offset, read_exact_at, SharedReader};
use crate::{ErofsError, ErofsSuperblock, Result};

use self::index::CompressionIndexes;
use self::map::{map_extent, ExtentStorage};

pub(crate) struct ErofsCompressedFile {
    source: SharedReader,
    volume_offset: u64,
    block_size: usize,
    block_count: u64,
    indexes: CompressionIndexes,
    size: u64,
    cache: Mutex<Option<CachedExtent>>,
}

struct CachedExtent {
    start: u64,
    end: u64,
    bytes: Arc<[u8]>,
}

impl ErofsCompressedFile {
    pub(crate) fn new(
        source: SharedReader,
        volume_offset: u64,
        superblock: &ErofsSuperblock,
        inode: &ErofsInode,
    ) -> Result<Self> {
        let indexes = CompressionIndexes::read(&source, volume_offset, superblock, inode)?;
        Ok(Self {
            source,
            volume_offset,
            block_size: superblock.block_size,
            block_count: superblock.block_count,
            indexes,
            size: inode.size,
            cache: Mutex::new(None),
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
            let position = offset.checked_add(written as u64).ok_or_else(|| {
                ErofsError::Invalid("compressed read position overflows".to_string())
            })?;
            let extent = self.load_extent(position)?;
            let within = usize::try_from(position - extent.start)
                .map_err(|_| ErofsError::Invalid("extent offset exceeds usize".to_string()))?;
            let available = extent.bytes.len().checked_sub(within).ok_or_else(|| {
                ErofsError::Invalid("compressed extent offset exceeds output".to_string())
            })?;
            let length = available.min(requested - written);
            output[written..written + length]
                .copy_from_slice(&extent.bytes[within..within + length]);
            written += length;
        }
        Ok(written)
    }

    fn load_extent(&self, position: u64) -> Result<CachedExtent> {
        if let Some(cached) = self
            .cache
            .lock()
            .map_err(|_| ErofsError::Invalid("compression cache lock is poisoned".to_string()))?
            .as_ref()
            .filter(|cached| cached.start <= position && position < cached.end)
            .map(|cached| CachedExtent {
                start: cached.start,
                end: cached.end,
                bytes: Arc::clone(&cached.bytes),
            })
        {
            return Ok(cached);
        }
        let mapped = map_extent(
            &self.indexes,
            &self.source,
            position,
            self.size,
            self.block_size,
        )?;
        let decoded_length = usize::try_from(mapped.end - mapped.start)
            .map_err(|_| ErofsError::Invalid("decoded extent length exceeds usize".to_string()))?;
        let bytes: Arc<[u8]> = match mapped.storage {
            ExtentStorage::Plain(block) => self.read_plain(block, decoded_length)?.into(),
            ExtentStorage::Lz4(block) => self.read_lz4(block, decoded_length)?.into(),
            ExtentStorage::Hole => vec![0u8; decoded_length].into(),
        };
        let cached = CachedExtent {
            start: mapped.start,
            end: mapped.end,
            bytes: Arc::clone(&bytes),
        };
        *self
            .cache
            .lock()
            .map_err(|_| ErofsError::Invalid("compression cache lock is poisoned".to_string()))? =
            Some(CachedExtent {
                start: cached.start,
                end: cached.end,
                bytes,
            });
        Ok(cached)
    }

    fn read_plain(&self, block: u64, decoded_length: usize) -> Result<Vec<u8>> {
        self.validate_physical_block(block)?;
        read_exact_at(
            &self.source,
            block_offset(self.volume_offset, block, self.block_size)?,
            decoded_length,
        )
    }

    fn read_lz4(&self, block: u64, decoded_length: usize) -> Result<Vec<u8>> {
        self.validate_physical_block(block)?;
        let encoded = read_exact_at(
            &self.source,
            block_offset(self.volume_offset, block, self.block_size)?,
            self.block_size,
        )?;
        let margin = encoded
            .iter()
            .position(|byte| *byte != 0)
            .ok_or_else(|| ErofsError::Invalid("empty LZ4 physical cluster".to_string()))?;
        let mut decoded = vec![0u8; decoded_length];
        let produced = lz4_flex::block::decompress_into(&encoded[margin..], &mut decoded)
            .map_err(|error| ErofsError::Invalid(format!("invalid LZ4 cluster: {error}")))?;
        if produced != decoded_length {
            return Err(ErofsError::Invalid(format!(
                "LZ4 cluster decoded {produced} bytes, expected {decoded_length}"
            )));
        }
        Ok(decoded)
    }

    fn validate_physical_block(&self, block: u64) -> Result<()> {
        if block >= self.block_count {
            return Err(ErofsError::Invalid(format!(
                "compressed data block {block} is outside filesystem"
            )));
        }
        Ok(())
    }
}
