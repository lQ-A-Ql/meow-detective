use std::sync::{Arc, Mutex};

use crate::inode::ErofsInode;
use crate::io::{block_offset, read_exact_at, read_u16, read_u32, SharedReader};
use crate::{ErofsError, ErofsSuperblock, Result};

const MAP_HEADER_BYTES: u64 = 8;
const FULL_INDEX_PADDING: u64 = 8;
const FULL_INDEX_BYTES: u64 = 8;
const LCLUSTER_TYPE_MASK: u16 = 0x0003;
const LCLUSTER_TYPE_PLAIN: u16 = 0;
const LCLUSTER_TYPE_HEAD1: u16 = 1;
const LCLUSTER_TYPE_NONHEAD: u16 = 2;
const LCLUSTER_TYPE_HEAD2: u16 = 3;
const LCLUSTER_HOLE: u16 = 1 << 14;
const MAP_HEADER_FRAGMENT: u8 = 1 << 7;

pub(crate) struct ErofsCompressedFile {
    source: SharedReader,
    volume_offset: u64,
    block_size: usize,
    block_count: u64,
    index_offset: u64,
    size: u64,
    cache: Mutex<Option<CachedCluster>>,
}

struct CachedCluster {
    index: u64,
    bytes: Arc<[u8]>,
}

enum ClusterKind {
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
        let metadata_end = inode
            .source_offset
            .checked_add(inode.inode_size as u64)
            .and_then(|offset| offset.checked_add(inode.xattr_size as u64))
            .ok_or_else(|| ErofsError::Invalid("compression map offset overflows".to_string()))?;
        let header_offset = align_up(metadata_end, 8)?;
        let header = read_exact_at(&source, header_offset, MAP_HEADER_BYTES as usize)?;
        validate_map_header(&header, inode.nid)?;
        let index_offset = header_offset
            .checked_add(MAP_HEADER_BYTES + FULL_INDEX_PADDING)
            .ok_or_else(|| ErofsError::Invalid("compression index offset overflows".to_string()))?;
        validate_index_table(
            index_offset,
            inode.size,
            superblock.block_size,
            volume_offset,
            superblock.block_count,
        )?;
        Ok(Self {
            source,
            volume_offset,
            block_size: superblock.block_size,
            block_count: superblock.block_count,
            index_offset,
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
        let offset = index
            .checked_mul(FULL_INDEX_BYTES)
            .and_then(|relative| self.index_offset.checked_add(relative))
            .ok_or_else(|| {
                ErofsError::Invalid("compression index address overflows".to_string())
            })?;
        let bytes = read_exact_at(&self.source, offset, FULL_INDEX_BYTES as usize)?;
        let advise = read_u16(&bytes, 0, "compressed cluster advice")?;
        if advise & !(LCLUSTER_TYPE_MASK | LCLUSTER_HOLE) != 0 {
            return Err(ErofsError::Unsupported(format!(
                "compressed cluster advice flags {:#x}",
                advise & !(LCLUSTER_TYPE_MASK | LCLUSTER_HOLE)
            )));
        }
        if read_u16(&bytes, 2, "compressed cluster offset")? != 0 {
            return Err(ErofsError::Unsupported(
                "non-zero compressed cluster offsets".to_string(),
            ));
        }
        if advise & LCLUSTER_HOLE != 0 {
            return Ok(ClusterKind::Hole);
        }
        let block = u64::from(read_u32(&bytes, 4, "compressed cluster block")?);
        match advise & LCLUSTER_TYPE_MASK {
            LCLUSTER_TYPE_PLAIN => Ok(ClusterKind::Plain(block)),
            LCLUSTER_TYPE_HEAD1 => Ok(ClusterKind::Lz4(block)),
            LCLUSTER_TYPE_NONHEAD => Err(ErofsError::Unsupported(
                "multi-cluster compressed extents".to_string(),
            )),
            LCLUSTER_TYPE_HEAD2 => Err(ErofsError::Unsupported(
                "secondary compression algorithms".to_string(),
            )),
            _ => Err(ErofsError::Invalid(
                "unreachable compressed cluster type".to_string(),
            )),
        }
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

fn validate_map_header(bytes: &[u8], nid: u64) -> Result<()> {
    if read_u16(bytes, 0, "compression map reserved field")? != 0 {
        return Err(ErofsError::Invalid(format!(
            "compression map reserved field is non-zero for inode {nid}"
        )));
    }
    if read_u16(bytes, 2, "compression inline data size")? != 0 {
        return Err(ErofsError::Unsupported(format!(
            "inline compressed tail data for inode {nid}"
        )));
    }
    let advise = read_u16(bytes, 4, "compression map advice")?;
    if advise != 0 {
        return Err(ErofsError::Unsupported(format!(
            "compression map advice {advise:#x} for inode {nid}"
        )));
    }
    let algorithms = *bytes
        .get(6)
        .ok_or_else(|| ErofsError::Invalid("truncated compression algorithms".to_string()))?;
    if algorithms != 0 {
        return Err(ErofsError::Unsupported(format!(
            "compression algorithms {algorithms:#x} for inode {nid}"
        )));
    }
    let cluster_bits = *bytes
        .get(7)
        .ok_or_else(|| ErofsError::Invalid("truncated compression cluster bits".to_string()))?;
    if cluster_bits & MAP_HEADER_FRAGMENT != 0 {
        return Err(ErofsError::Unsupported(
            "fragment-packed compressed file".to_string(),
        ));
    }
    if cluster_bits != 0 {
        return Err(ErofsError::Unsupported(format!(
            "compression logical cluster bits {cluster_bits:#x}"
        )));
    }
    Ok(())
}

fn validate_index_table(
    index_offset: u64,
    size: u64,
    block_size: usize,
    volume_offset: u64,
    block_count: u64,
) -> Result<()> {
    let index_end = size
        .div_ceil(block_size as u64)
        .checked_mul(FULL_INDEX_BYTES)
        .and_then(|bytes| index_offset.checked_add(bytes))
        .ok_or_else(|| ErofsError::Invalid("compression index table overflows".to_string()))?;
    let filesystem_end = block_count
        .checked_mul(block_size as u64)
        .and_then(|bytes| volume_offset.checked_add(bytes))
        .ok_or_else(|| ErofsError::Invalid("filesystem end offset overflows".to_string()))?;
    if index_end > filesystem_end {
        return Err(ErofsError::Invalid(
            "compression index table exceeds filesystem".to_string(),
        ));
    }
    Ok(())
}

fn align_up(value: u64, alignment: u64) -> Result<u64> {
    value
        .checked_add(alignment - 1)
        .map(|sum| sum / alignment * alignment)
        .ok_or_else(|| ErofsError::Invalid("compression map alignment overflows".to_string()))
}
