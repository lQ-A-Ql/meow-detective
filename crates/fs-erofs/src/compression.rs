mod index;

use std::sync::{Arc, Mutex};

use crate::inode::ErofsInode;
use crate::io::{block_offset, read_exact_at, SharedReader};
use crate::{ErofsError, ErofsSuperblock, Result};

use self::index::CompressionIndexes;

pub(crate) struct ErofsCompressedFile {
    source: SharedReader,
    volume_offset: u64,
    block_size: usize,
    block_count: u64,
    indexes: CompressionIndexes,
    size: u64,
    cache: Mutex<Option<CachedCluster>>,
}

struct CachedCluster {
    index: u64,
    bytes: Arc<[u8]>,
}

pub(super) enum ClusterKind {
    Plain(u64),
    Lz4(u64),
    Hole,
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
            let cluster_index = position / self.block_size as u64;
            let within = usize::try_from(position % self.block_size as u64)
                .map_err(|_| ErofsError::Invalid("cluster offset exceeds usize".to_string()))?;
            let cluster = self.load_cluster(cluster_index)?;
            let available = cluster.len().checked_sub(within).ok_or_else(|| {
                ErofsError::Invalid("compressed cluster offset exceeds output".to_string())
            })?;
            let length = available.min(requested - written);
            output[written..written + length].copy_from_slice(&cluster[within..within + length]);
            written += length;
        }
        Ok(written)
    }

    fn load_cluster(&self, index: u64) -> Result<Arc<[u8]>> {
        if let Some(bytes) = self
            .cache
            .lock()
            .map_err(|_| ErofsError::Invalid("compression cache lock is poisoned".to_string()))?
            .as_ref()
            .filter(|cached| cached.index == index)
            .map(|cached| Arc::clone(&cached.bytes))
        {
            return Ok(bytes);
        }
        let cluster_start = index
            .checked_mul(self.block_size as u64)
            .ok_or_else(|| ErofsError::Invalid("logical cluster offset overflows".to_string()))?;
        let remaining = self.size.checked_sub(cluster_start).ok_or_else(|| {
            ErofsError::Invalid(format!("logical cluster {index} is beyond the file"))
        })?;
        let decoded_length = usize::try_from(remaining.min(self.block_size as u64))
            .map_err(|_| ErofsError::Invalid("decoded cluster length exceeds usize".to_string()))?;
        let bytes: Arc<[u8]> = match self.read_cluster_kind(index)? {
            ClusterKind::Plain(block) => self.read_plain(block, decoded_length)?.into(),
            ClusterKind::Lz4(block) => self.read_lz4(block, decoded_length)?.into(),
            ClusterKind::Hole => vec![0u8; decoded_length].into(),
        };
        *self
            .cache
            .lock()
            .map_err(|_| ErofsError::Invalid("compression cache lock is poisoned".to_string()))? =
            Some(CachedCluster {
                index,
                bytes: Arc::clone(&bytes),
            });
        Ok(bytes)
    }

    fn read_cluster_kind(&self, index: u64) -> Result<ClusterKind> {
        self.indexes.read_cluster_kind(&self.source, index)
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
