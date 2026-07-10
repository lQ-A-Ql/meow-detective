use std::collections::HashMap;
use std::io::SeekFrom;
use std::sync::{Arc, Mutex, MutexGuard};

use evidence_core::EvidenceReader;

use crate::error::{LvmError, Result};

const METADATA_BLOCK_SIZE: usize = 4096;
const METADATA_BLOCK_SIZE_U64: u64 = METADATA_BLOCK_SIZE as u64;
const THIN_SUPERBLOCK_MAGIC: u64 = 27_022_010;
const THIN_SUPERBLOCK_LOCATION: u64 = 0;
const NODE_HEADER_SIZE: usize = 32;
const INTERNAL_NODE_FLAG: u32 = 1;
const LEAF_NODE_FLAG: u32 = 2;
const BLOCK_TIME_VALUE_SIZE: u32 = 8;
const DEVICE_DETAIL_VALUE_SIZE: u32 = 24;
const U64_VALUE_SIZE: u32 = 8;

type SharedReader = Arc<Mutex<Box<dyn EvidenceReader>>>;

#[derive(Debug, Clone)]
pub struct ThinSuperblock {
    pub transaction_id: u64,
    pub mapping_root: u64,
    pub details_root: u64,
    pub data_block_size_sectors: u32,
    pub nr_metadata_blocks: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockTime {
    pub block: u64,
    pub time: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceDetail {
    pub mapped_blocks: u64,
    pub transaction_id: u64,
    pub creation_time: u32,
    pub snapshotted_time: u32,
}

#[derive(Debug, Clone, Copy)]
struct NodeHeader {
    is_leaf: bool,
    nr_entries: u32,
    max_entries: u32,
    value_size: u32,
}

pub struct ThinMetadata {
    reader: SharedReader,
    superblock: ThinSuperblock,
    mapping_roots: Mutex<HashMap<u64, u64>>,
}

impl ThinMetadata {
    pub fn open(reader: Box<dyn EvidenceReader>) -> Result<Self> {
        let reader = Arc::new(Mutex::new(reader));
        let superblock = {
            let mut guard = lock_mutex(&reader, "thin metadata reader")?;
            read_superblock(&mut **guard)?
        };
        Ok(Self {
            reader,
            superblock,
            mapping_roots: Mutex::new(HashMap::new()),
        })
    }

    pub fn superblock(&self) -> &ThinSuperblock {
        &self.superblock
    }

    pub fn data_block_size_bytes(&self) -> Result<u64> {
        checked_mul(
            self.superblock.data_block_size_sectors as u64,
            512,
            "thin data block size",
        )
    }

    pub fn device_detail(&self, device_id: u64) -> Result<Option<DeviceDetail>> {
        let mut reader = lock_mutex(&self.reader, "thin metadata reader")?;
        btree_lookup_device_detail(&mut **reader, self.superblock.details_root, device_id)
    }

    pub fn lookup_data_block(&self, device_id: u64, thin_block: u64) -> Result<Option<BlockTime>> {
        let mapping_root = self.mapping_root_for_device(device_id)?;
        let Some(mapping_root) = mapping_root else {
            return Ok(None);
        };
        let mut reader = lock_mutex(&self.reader, "thin metadata reader")?;
        btree_lookup_block_time(&mut **reader, mapping_root, thin_block)
    }

    fn mapping_root_for_device(&self, device_id: u64) -> Result<Option<u64>> {
        {
            let mapping_roots = lock_mutex(&self.mapping_roots, "thin mapping root cache")?;
            if let Some(root) = mapping_roots.get(&device_id).copied() {
                return Ok(Some(root));
            }
        }
        let mut reader = lock_mutex(&self.reader, "thin metadata reader")?;
        let root = btree_lookup_u64(&mut **reader, self.superblock.mapping_root, device_id)?;
        if let Some(root) = root {
            lock_mutex(&self.mapping_roots, "thin mapping root cache")?.insert(device_id, root);
        }
        Ok(root)
    }
}

fn read_superblock(reader: &mut dyn EvidenceReader) -> Result<ThinSuperblock> {
    let block = read_metadata_block(reader, THIN_SUPERBLOCK_LOCATION)?;
    let magic = read_u64(&block, 32, "thin superblock magic")?;
    if magic != THIN_SUPERBLOCK_MAGIC {
        return Err(metadata_error(format!(
            "thin metadata superblock magic mismatch: expected {THIN_SUPERBLOCK_MAGIC}, got {magic}"
        )));
    }
    let block_number = read_u64(&block, 8, "thin superblock block number")?;
    if block_number != THIN_SUPERBLOCK_LOCATION {
        return Err(metadata_error(format!(
            "thin metadata superblock stored block number {block_number}, expected 0"
        )));
    }
    let mapping_root = read_u64(&block, 320, "thin mapping root")?;
    let details_root = read_u64(&block, 328, "thin details root")?;
    let data_block_size_sectors = read_u32(&block, 336, "thin data block size")?;
    let metadata_block_size_sectors = read_u32(&block, 340, "thin metadata block size")?;
    let nr_metadata_blocks = read_u64(&block, 344, "thin metadata block count")?;
    if metadata_block_size_sectors != 8 {
        return Err(metadata_error(format!(
            "unsupported thin metadata block size {metadata_block_size_sectors} sectors"
        )));
    }
    if data_block_size_sectors == 0 {
        return Err(metadata_error(
            "thin metadata data_block_size is zero".to_string(),
        ));
    }
    if mapping_root == 0 || details_root == 0 {
        return Err(metadata_error(
            "thin metadata roots must be non-zero".to_string(),
        ));
    }
    if nr_metadata_blocks > 0
        && (mapping_root >= nr_metadata_blocks || details_root >= nr_metadata_blocks)
    {
        return Err(metadata_error(format!(
            "thin metadata root outside metadata device: mapping_root={mapping_root}, details_root={details_root}, nr_metadata_blocks={nr_metadata_blocks}"
        )));
    }

    Ok(ThinSuperblock {
        transaction_id: read_u64(&block, 48, "thin transaction id")?,
        mapping_root,
        details_root,
        data_block_size_sectors,
        nr_metadata_blocks,
    })
}

fn btree_lookup_u64(reader: &mut dyn EvidenceReader, root: u64, key: u64) -> Result<Option<u64>> {
    btree_lookup(reader, root, key, U64_VALUE_SIZE, parse_u64_value)
}

fn btree_lookup_block_time(
    reader: &mut dyn EvidenceReader,
    root: u64,
    key: u64,
) -> Result<Option<BlockTime>> {
    btree_lookup(
        reader,
        root,
        key,
        BLOCK_TIME_VALUE_SIZE,
        parse_block_time_value,
    )
}

fn btree_lookup_device_detail(
    reader: &mut dyn EvidenceReader,
    root: u64,
    key: u64,
) -> Result<Option<DeviceDetail>> {
    btree_lookup(
        reader,
        root,
        key,
        DEVICE_DETAIL_VALUE_SIZE,
        parse_device_detail_value,
    )
}

fn btree_lookup<T, F>(
    reader: &mut dyn EvidenceReader,
    root: u64,
    key: u64,
    leaf_value_size: u32,
    parse_value: F,
) -> Result<Option<T>>
where
    F: Fn(&[u8]) -> Result<T>,
{
    let mut loc = root;
    let mut is_root = true;
    let mut visited = Vec::new();

    for _depth in 0..64 {
        if visited.contains(&loc) {
            return Err(metadata_error(format!(
                "thin metadata btree contains a cycle at block {loc}"
            )));
        }
        visited.push(loc);

        let block = read_metadata_block(reader, loc)?;
        let header = parse_node_header(&block, loc, leaf_value_size, is_root)?;
        let keys = parse_node_keys(&block, &header)?;

        if header.is_leaf {
            return match keys.binary_search(&key) {
                Ok(idx) => parse_leaf_value(&block, &header, idx, parse_value).map(Some),
                Err(_) => Ok(None),
            };
        }
        let idx = match keys.binary_search(&key) {
            Ok(idx) => idx,
            Err(idx) => {
                if idx == 0 {
                    return Ok(None);
                }
                idx - 1
            }
        };
        loc = parse_internal_child(&block, &header, idx)?;
        is_root = false;
    }

    Err(metadata_error(
        "thin metadata btree exceeds maximum traversal depth".to_string(),
    ))
}

fn parse_node_header(
    block: &[u8; METADATA_BLOCK_SIZE],
    loc: u64,
    expected_leaf_value_size: u32,
    is_root: bool,
) -> Result<NodeHeader> {
    let flags = read_u32(block, 4, "thin btree node flags")?;
    let is_leaf = match flags {
        INTERNAL_NODE_FLAG => false,
        LEAF_NODE_FLAG => true,
        _ => {
            return Err(metadata_error(format!(
                "thin metadata node {loc} has invalid flags {flags}"
            )));
        }
    };
    let stored_block = read_u64(block, 8, "thin btree node block")?;
    if stored_block != loc {
        return Err(metadata_error(format!(
            "thin metadata node block mismatch: node says {stored_block}, read {loc}"
        )));
    }
    let nr_entries = read_u32(block, 16, "thin btree node entries")?;
    let max_entries = read_u32(block, 20, "thin btree node max entries")?;
    let value_size = read_u32(block, 24, "thin btree node value size")?;
    if max_entries == 0 {
        return Err(metadata_error(format!(
            "thin metadata node {loc} has zero max_entries"
        )));
    }
    if nr_entries > max_entries {
        return Err(metadata_error(format!(
            "thin metadata node {loc} entries {nr_entries} exceed max_entries {max_entries}"
        )));
    }
    if is_leaf && value_size != expected_leaf_value_size {
        return Err(metadata_error(format!(
            "thin metadata leaf {loc} value_size {value_size} does not match expected {expected_leaf_value_size}"
        )));
    }
    let values_offset = values_offset(max_entries)?;
    let value_bytes = if is_leaf {
        checked_mul(
            nr_entries as u64,
            value_size as u64,
            "thin leaf value bytes",
        )?
    } else {
        checked_mul(nr_entries as u64, 8, "thin internal child bytes")?
    };
    if checked_add(values_offset as u64, value_bytes, "thin node payload end")?
        > METADATA_BLOCK_SIZE_U64
    {
        return Err(metadata_error(format!(
            "thin metadata node {loc} payload exceeds metadata block"
        )));
    }
    if !is_root && !is_leaf && nr_entries == 0 {
        return Err(metadata_error(format!(
            "thin metadata internal node {loc} has no entries"
        )));
    }

    Ok(NodeHeader {
        is_leaf,
        nr_entries,
        max_entries,
        value_size,
    })
}

fn parse_node_keys(block: &[u8; METADATA_BLOCK_SIZE], header: &NodeHeader) -> Result<Vec<u64>> {
    let mut keys = Vec::with_capacity(header.nr_entries as usize);
    let mut cursor = NODE_HEADER_SIZE;
    for _ in 0..header.nr_entries {
        keys.push(read_u64(block, cursor, "thin btree key")?);
        cursor += 8;
    }
    if !keys.windows(2).all(|window| window[0] < window[1]) {
        return Err(metadata_error(
            "thin metadata btree keys are not strictly increasing".to_string(),
        ));
    }
    Ok(keys)
}

fn parse_internal_child(
    block: &[u8; METADATA_BLOCK_SIZE],
    header: &NodeHeader,
    idx: usize,
) -> Result<u64> {
    let offset = values_offset(header.max_entries)?
        .checked_add(idx.checked_mul(8).ok_or_else(|| {
            metadata_error("thin metadata child offset overflows usize".to_string())
        })?)
        .ok_or_else(|| metadata_error("thin metadata child offset overflows usize".to_string()))?;
    read_u64(block, offset, "thin btree child")
}

fn parse_leaf_value<T, F>(
    block: &[u8; METADATA_BLOCK_SIZE],
    header: &NodeHeader,
    idx: usize,
    parse_value: F,
) -> Result<T>
where
    F: Fn(&[u8]) -> Result<T>,
{
    let value_size = header.value_size as usize;
    let start = values_offset(header.max_entries)?
        .checked_add(idx.checked_mul(value_size).ok_or_else(|| {
            metadata_error("thin metadata value offset overflows usize".to_string())
        })?)
        .ok_or_else(|| metadata_error("thin metadata value offset overflows usize".to_string()))?;
    let end = start
        .checked_add(value_size)
        .ok_or_else(|| metadata_error("thin metadata value end overflows usize".to_string()))?;
    if end > METADATA_BLOCK_SIZE {
        return Err(metadata_error(
            "thin metadata value exceeds metadata block".to_string(),
        ));
    }
    parse_value(&block[start..end])
}

fn values_offset(max_entries: u32) -> Result<usize> {
    let key_bytes = (max_entries as usize)
        .checked_mul(8)
        .ok_or_else(|| metadata_error("thin metadata key area overflows usize".to_string()))?;
    NODE_HEADER_SIZE
        .checked_add(key_bytes)
        .ok_or_else(|| metadata_error("thin metadata value area overflows usize".to_string()))
}

fn parse_u64_value(data: &[u8]) -> Result<u64> {
    read_u64(data, 0, "thin u64 value")
}

fn parse_block_time_value(data: &[u8]) -> Result<BlockTime> {
    let raw = read_u64(data, 0, "thin block_time value")?;
    Ok(BlockTime {
        block: raw >> 24,
        time: (raw & ((1 << 24) - 1)) as u32,
    })
}

fn parse_device_detail_value(data: &[u8]) -> Result<DeviceDetail> {
    Ok(DeviceDetail {
        mapped_blocks: read_u64(data, 0, "thin device mapped_blocks")?,
        transaction_id: read_u64(data, 8, "thin device transaction_id")?,
        creation_time: read_u32(data, 16, "thin device creation_time")?,
        snapshotted_time: read_u32(data, 20, "thin device snapshotted_time")?,
    })
}

fn read_metadata_block(
    reader: &mut dyn EvidenceReader,
    block: u64,
) -> Result<[u8; METADATA_BLOCK_SIZE]> {
    let offset = checked_mul(block, METADATA_BLOCK_SIZE_U64, "thin metadata block offset")?;
    let mut buf = [0u8; METADATA_BLOCK_SIZE];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

fn read_u32(data: &[u8], offset: usize, context: &str) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| metadata_error(format!("{context} offset overflows usize")))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| metadata_error(format!("{context} is outside buffer")))?;
    let bytes = bytes
        .try_into()
        .map_err(|_| metadata_error(format!("{context} must be exactly 4 bytes")))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(data: &[u8], offset: usize, context: &str) -> Result<u64> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| metadata_error(format!("{context} offset overflows usize")))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| metadata_error(format!("{context} is outside buffer")))?;
    let bytes = bytes
        .try_into()
        .map_err(|_| metadata_error(format!("{context} must be exactly 8 bytes")))?;
    Ok(u64::from_le_bytes(bytes))
}

fn checked_add(lhs: u64, rhs: u64, context: &str) -> Result<u64> {
    lhs.checked_add(rhs)
        .ok_or_else(|| metadata_error(format!("{context} overflows u64")))
}

fn checked_mul(lhs: u64, rhs: u64, context: &str) -> Result<u64> {
    lhs.checked_mul(rhs)
        .ok_or_else(|| metadata_error(format!("{context} overflows u64")))
}

fn metadata_error(message: String) -> LvmError {
    LvmError::MetadataParseError { line: 0, message }
}

fn lock_mutex<'a, T>(mutex: &'a Mutex<T>, context: &str) -> Result<MutexGuard<'a, T>> {
    mutex
        .lock()
        .map_err(|_| metadata_error(format!("{context} lock poisoned")))
}
