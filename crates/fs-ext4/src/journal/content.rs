use super::error::{require_len, JournalError, JournalResult};
use super::recovery::RecoveryCompleteness;
use super::{content_allocation, content_builder::MappingBuilder};
use crate::format::{Ext4Extent, Ext4ExtentHeader, I_BLOCK_SIZE};
use crate::Ext4Reader;

const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
const INODE_FLAGS_OFFSET: usize = 0x20;
const INODE_BLOCK_OFFSET: usize = 0x28;
const EXTENT_HEADER_SIZE: usize = 12;
const EXTENT_RECORD_SIZE: usize = 12;
const MAX_RECOVERY_CONTENT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryAllocationState {
    Unverified,
    Free,
    Allocated,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeletedContentMappingState {
    Mapped,
    Unsupported,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeletedContentRangeKind {
    RecoverableData,
    AllocatedData,
    UnreadableData,
    Sparse,
    Unwritten,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedContentRange {
    pub logical_offset: u64,
    pub filesystem_block: Option<u64>,
    /// Byte offset in the filesystem reader's source view. For an LVM-backed
    /// reader this is not necessarily a physical evidence-container offset.
    pub filesystem_source_offset: Option<u64>,
    pub length: u64,
    pub kind: DeletedContentRangeKind,
    pub allocation_state: RecoveryAllocationState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedContentMapping {
    pub state: DeletedContentMappingState,
    pub inode_allocation_state: RecoveryAllocationState,
    pub data_allocation_state: RecoveryAllocationState,
    pub ranges: Vec<DeletedContentRange>,
    pub recoverable_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
}

impl DeletedContentMapping {
    pub(crate) fn metadata_only() -> Self {
        Self {
            state: DeletedContentMappingState::Unsupported,
            inode_allocation_state: RecoveryAllocationState::Unverified,
            data_allocation_state: RecoveryAllocationState::Unverified,
            ranges: Vec::new(),
            recoverable_bytes: 0,
            content_sha256: None,
            issue: None,
        }
    }

    pub(crate) fn unavailable(error: &JournalError) -> Self {
        let state = match error {
            JournalError::Unsupported(_) => DeletedContentMappingState::Unsupported,
            _ => DeletedContentMappingState::Invalid,
        };
        Self {
            state,
            inode_allocation_state: RecoveryAllocationState::Unverified,
            data_allocation_state: RecoveryAllocationState::Unverified,
            ranges: Vec::new(),
            recoverable_bytes: 0,
            content_sha256: None,
            issue: Some(error.to_string()),
        }
    }

    pub(crate) fn completeness(&self, declared_size: u64) -> RecoveryCompleteness {
        if self.inode_allocation_state != RecoveryAllocationState::Free {
            RecoveryCompleteness::MetadataOnly
        } else if (declared_size == 0
            && self.state == DeletedContentMappingState::Mapped
            && self.issue.is_none())
            || (declared_size != 0 && self.recoverable_bytes == declared_size)
        {
            RecoveryCompleteness::Complete
        } else if self.recoverable_bytes != 0 {
            RecoveryCompleteness::Partial
        } else {
            RecoveryCompleteness::MetadataOnly
        }
    }
}

pub(crate) fn map_deleted_inode_content(
    filesystem: &Ext4Reader,
    inode_number: u32,
    inode: &[u8],
    declared_size: u64,
) -> JournalResult<DeletedContentMapping> {
    if filesystem.has_bigalloc {
        return Err(JournalError::Unsupported(
            "deleted-content bitmap validation does not support ext4 bigalloc".into(),
        ));
    }
    if declared_size > MAX_RECOVERY_CONTENT_BYTES {
        return Err(JournalError::Unsupported(format!(
            "deleted-content validation limit is {MAX_RECOVERY_CONTENT_BYTES} bytes, file declares {declared_size} bytes"
        )));
    }
    require_len(
        inode,
        INODE_BLOCK_OFFSET + I_BLOCK_SIZE,
        "deleted inode extent record",
    )?;
    let flags = read_le_u32(inode, INODE_FLAGS_OFFSET, "deleted inode flags")?;
    if flags & EXT4_EXTENTS_FL == 0 {
        return Err(JournalError::Unsupported(
            "deleted inode does not use the ext4 extent format".into(),
        ));
    }
    let inode_allocation_state = content_allocation::inode_allocation(filesystem, inode_number)?;
    if inode_allocation_state != RecoveryAllocationState::Free {
        return Ok(DeletedContentMapping {
            state: DeletedContentMappingState::Mapped,
            inode_allocation_state,
            data_allocation_state: RecoveryAllocationState::Unverified,
            ranges: Vec::new(),
            recoverable_bytes: 0,
            content_sha256: None,
            issue: Some(
                "the inode is currently allocated, so its historical extent mapping was not trusted"
                    .to_string(),
            ),
        });
    }
    let extents = parse_direct_extents(&inode[INODE_BLOCK_OFFSET..], filesystem, declared_size)?;
    let mut builder = MappingBuilder::new(inode_allocation_state);
    let mut next_logical_offset = 0u64;

    for extent in extents {
        if next_logical_offset < extent.logical_offset {
            builder.push_sparse(
                next_logical_offset,
                extent.logical_offset - next_logical_offset,
            )?;
        }
        map_extent(filesystem, &extent, declared_size, &mut builder)?;
        next_logical_offset = extent.logical_end;
    }
    if next_logical_offset < declared_size {
        builder.push_sparse(next_logical_offset, declared_size - next_logical_offset)?;
    }
    Ok(builder.finish())
}

#[derive(Debug)]
struct DirectExtent {
    logical_offset: u64,
    logical_end: u64,
    physical_start: u64,
    block_count: u64,
    unwritten: bool,
}

fn parse_direct_extents(
    i_block: &[u8],
    filesystem: &Ext4Reader,
    declared_size: u64,
) -> JournalResult<Vec<DirectExtent>> {
    require_len(i_block, EXTENT_HEADER_SIZE, "deleted inode extent header")?;
    let header = Ext4ExtentHeader::parse(i_block)?;
    if header.eh_depth != 0 {
        return Err(JournalError::Unsupported(format!(
            "deleted-content mapping supports extent depth 0, found {}",
            header.eh_depth
        )));
    }
    let header_max = read_le_u16(i_block, 4, "deleted inode extent maximum")?;
    let capacity = (i_block.len() - EXTENT_HEADER_SIZE) / EXTENT_RECORD_SIZE;
    if usize::from(header.eh_entries) > capacity || header.eh_entries > header_max {
        return Err(JournalError::Invalid(format!(
            "deleted inode extent count {} exceeds record capacity {} or header maximum {}",
            header.eh_entries, capacity, header_max
        )));
    }

    let mut result = Vec::with_capacity(usize::from(header.eh_entries));
    let mut previous_end = 0u64;
    for index in 0..usize::from(header.eh_entries) {
        let start = EXTENT_HEADER_SIZE
            .checked_add(index.checked_mul(EXTENT_RECORD_SIZE).ok_or_else(|| {
                JournalError::Invalid("deleted inode extent index overflows".into())
            })?)
            .ok_or_else(|| JournalError::Invalid("deleted inode extent offset overflows".into()))?;
        let end = start
            .checked_add(EXTENT_RECORD_SIZE)
            .ok_or_else(|| JournalError::Invalid("deleted inode extent end overflows".into()))?;
        let extent = Ext4Extent::parse(&i_block[start..end])?;
        let block_count = u64::from(extent.block_count());
        if block_count == 0 {
            return Err(JournalError::Invalid(
                "deleted inode contains a zero-length extent".into(),
            ));
        }
        let logical_offset = u64::from(extent.ee_block)
            .checked_mul(filesystem.block_size)
            .ok_or_else(|| JournalError::Invalid("extent logical offset overflows".into()))?;
        let extent_bytes = block_count
            .checked_mul(filesystem.block_size)
            .ok_or_else(|| JournalError::Invalid("extent byte length overflows".into()))?;
        let logical_end = logical_offset
            .checked_add(extent_bytes)
            .ok_or_else(|| JournalError::Invalid("extent logical end overflows".into()))?
            .min(declared_size);
        if logical_offset < previous_end {
            return Err(JournalError::Invalid(
                "deleted inode extents overlap or are not ordered".into(),
            ));
        }
        let physical_start = (u64::from(extent.ee_start_hi) << 32) | u64::from(extent.ee_start_lo);
        let physical_end = physical_start
            .checked_add(block_count)
            .ok_or_else(|| JournalError::Invalid("extent physical range overflows".into()))?;
        if physical_start < filesystem.first_data_block || physical_end > filesystem.blocks_count {
            return Err(JournalError::Invalid(format!(
                "deleted inode extent blocks {physical_start}..{physical_end} exceed filesystem bounds"
            )));
        }
        if logical_offset < declared_size {
            result.push(DirectExtent {
                logical_offset,
                logical_end,
                physical_start,
                block_count,
                unwritten: extent.is_unwritten(),
            });
            previous_end = logical_end;
        }
    }
    Ok(result)
}

fn map_extent(
    filesystem: &Ext4Reader,
    extent: &DirectExtent,
    declared_size: u64,
    builder: &mut MappingBuilder,
) -> JournalResult<()> {
    for relative_block in 0..extent.block_count {
        let logical_offset = extent
            .logical_offset
            .checked_add(
                relative_block
                    .checked_mul(filesystem.block_size)
                    .ok_or_else(|| {
                        JournalError::Invalid("extent logical block offset overflows".into())
                    })?,
            )
            .ok_or_else(|| JournalError::Invalid("extent logical block overflows".into()))?;
        if logical_offset >= declared_size || logical_offset >= extent.logical_end {
            break;
        }
        let length = filesystem
            .block_size
            .min(declared_size - logical_offset)
            .min(extent.logical_end - logical_offset);
        let block = extent
            .physical_start
            .checked_add(relative_block)
            .ok_or_else(|| JournalError::Invalid("extent data block overflows".into()))?;
        let allocation = content_allocation::block_allocation(filesystem, block)?;
        let source_offset = filesystem.block_to_offset(block)?;
        builder.observe_allocation(allocation);
        if extent.unwritten {
            builder.push_range(DeletedContentRange {
                logical_offset,
                filesystem_block: Some(block),
                filesystem_source_offset: Some(source_offset),
                length,
                kind: DeletedContentRangeKind::Unwritten,
                allocation_state: allocation,
                sha256: None,
            })?;
            continue;
        }
        let (kind, sha256, content) = match allocation {
            RecoveryAllocationState::Allocated => {
                (DeletedContentRangeKind::AllocatedData, None, None)
            }
            RecoveryAllocationState::Free => match filesystem.read_block(block) {
                Ok(bytes) => {
                    let content_length = usize::try_from(length).map_err(|_| {
                        JournalError::Invalid("recoverable block length exceeds usize".into())
                    })?;
                    let content = bytes.get(..content_length).ok_or_else(|| {
                        JournalError::Invalid("recoverable block is shorter than its range".into())
                    })?;
                    (
                        DeletedContentRangeKind::RecoverableData,
                        Some(super::checksum::sha256_hex(content)),
                        Some(content.to_vec()),
                    )
                }
                Err(_) => (DeletedContentRangeKind::UnreadableData, None, None),
            },
            _ => {
                return Err(JournalError::Invalid(
                    "individual block allocation was not free or allocated".into(),
                ))
            }
        };
        if let Some(content) = content.as_deref() {
            builder.observe_recoverable_content(logical_offset, content)?;
        }
        builder.push_range(DeletedContentRange {
            logical_offset,
            filesystem_block: Some(block),
            filesystem_source_offset: Some(source_offset),
            length,
            kind,
            allocation_state: allocation,
            sha256,
        })?;
    }
    Ok(())
}

fn read_le_u16(data: &[u8], offset: usize, context: &'static str) -> JournalResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| JournalError::Invalid(format!("{context} offset overflows")))?;
    let bytes = data.get(offset..end).ok_or(JournalError::Truncated {
        context,
        needed: end,
        available: data.len(),
    })?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_le_u32(data: &[u8], offset: usize, context: &'static str) -> JournalResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| JournalError::Invalid(format!("{context} offset overflows")))?;
    let bytes = data.get(offset..end).ok_or(JournalError::Truncated {
        context,
        needed: end,
        available: data.len(),
    })?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        JournalError::Invalid(format!("{context} is invalid"))
    })?))
}
