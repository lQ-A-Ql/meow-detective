use super::content::{map_deleted_inode_content, DeletedContentMapping};
use super::error::{JournalError, JournalResult};
use super::inode_checksum::verify_inode_checksum;
use super::ring::{journal_block_data, parse_journal_history};
use super::types::{
    JournalBlockMapping, JournalSuperblock, JBD2_FLAG_DELETED, JBD2_FLAG_ESCAPE, JBD2_MAGIC_NUMBER,
};
use crate::format::inode_table_block_from_descriptor;
use crate::Ext4Reader;
use std::borrow::Cow;

const EXT4_S_IFMT: u16 = 0xF000;
const EXT4_S_IFREG: u16 = 0x8000;
const EXT4_S_IFDIR: u16 = 0x4000;
const EXT4_S_IFLNK: u16 = 0xA000;
const MIN_EXT4_INODE_SIZE: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryCompleteness {
    MetadataOnly,
    Partial,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeletedInodeKind {
    RegularFile,
    Directory,
    SymbolicLink,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedInodeCandidate {
    pub inode: u32,
    pub kind: DeletedInodeKind,
    pub mode: u16,
    pub declared_size: u64,
    pub deletion_time: u32,
    pub transaction_sequence: u32,
    pub descriptor_journal_block: u32,
    pub payload_journal_block: u32,
    pub inode_offset_within_payload: u32,
    /// Byte offset relative to the start of the journal inode/device source.
    /// This is not an evidence-container physical offset.
    pub journal_source_offset: u64,
    /// Length of the inode record at `journal_source_offset`.
    pub journal_source_length: u32,
    pub inode_table_group: u32,
    pub inode_table_block: u64,
    pub tag_marked_deleted: bool,
    pub replay_revoked: bool,
    pub journal_checksum_verified: bool,
    pub inode_checksum_verified: bool,
    pub completeness: RecoveryCompleteness,
    pub recoverable_bytes: u64,
    pub content_mapping: DeletedContentMapping,
    pub confidence: f64,
    pub recovery_method: String,
}

pub fn recover_deleted_inodes(
    filesystem: &Ext4Reader,
    journal_data: &[u8],
) -> JournalResult<Vec<DeletedInodeCandidate>> {
    let history = parse_journal_history(journal_data)?;
    if u64::from(history.superblock.block_size) != filesystem.block_size {
        return Err(JournalError::Invalid(format!(
            "journal block size {} differs from filesystem block size {}",
            history.superblock.block_size, filesystem.block_size
        )));
    }
    let layouts = inode_table_layouts(filesystem)?;
    let mut candidates = Vec::new();
    for transaction in &history.transactions {
        for mapping in &transaction.mappings {
            if mapping.uuid != filesystem.filesystem_uuid {
                continue;
            }
            let Some(layout) = find_inode_table_layout(&layouts, mapping.target_filesystem_block)
            else {
                continue;
            };
            recover_mapping_candidates(
                filesystem,
                journal_data,
                &history.superblock,
                mapping,
                layout,
                &mut candidates,
            )?;
        }
    }
    candidates.sort_by_key(|candidate| {
        (
            candidate.transaction_sequence,
            candidate.inode,
            candidate.payload_journal_block,
        )
    });
    candidates.dedup_by_key(|candidate| {
        (
            candidate.transaction_sequence,
            candidate.inode,
            candidate.payload_journal_block,
        )
    });
    Ok(candidates)
}

fn recover_mapping_candidates(
    filesystem: &Ext4Reader,
    journal_data: &[u8],
    superblock: &JournalSuperblock,
    mapping: &JournalBlockMapping,
    layout: &InodeTableLayout,
    candidates: &mut Vec<DeletedInodeCandidate>,
) -> JournalResult<()> {
    let raw_payload = journal_block_data(journal_data, superblock, mapping.payload_journal_block)?;
    let payload = unescaped_payload(raw_payload, mapping.flags);
    let inode_size = usize::from(filesystem.inode_size);
    let inodes_per_block = payload.len() / inode_size;
    let table_block_index = mapping.target_filesystem_block - layout.start_block;
    let first_local_inode = table_block_index
        .checked_mul(inodes_per_block as u64)
        .ok_or_else(|| JournalError::Invalid("inode-table index overflows".into()))?;

    for slot in 0..inodes_per_block {
        let local_inode = first_local_inode + slot as u64;
        if local_inode >= u64::from(layout.inode_count) {
            break;
        }
        let start = slot * inode_size;
        let inode = &payload[start..start + inode_size];
        // The tag DELETED bit alone does not prove that these bytes are a deleted inode.
        let Some((kind, mode, deletion_time)) = deleted_inode_metadata(inode) else {
            continue;
        };
        let inode_number = u64::from(layout.group)
            .checked_mul(u64::from(filesystem.inodes_per_group))
            .and_then(|base| base.checked_add(local_inode + 1))
            .and_then(|inode| u32::try_from(inode).ok())
            .ok_or_else(|| JournalError::Invalid("recovered inode number overflows".into()))?;
        if inode_number > filesystem.inodes_count {
            break;
        }
        let checksum_verified = superblock.uses_v2_or_v3_checksums();
        let payload_source_offset = u64::from(mapping.payload_journal_block)
            .checked_mul(u64::from(superblock.block_size))
            .ok_or_else(|| JournalError::Invalid("journal source offset overflows".into()))?;
        let inode_offset_within_payload = u32::try_from(start)
            .map_err(|_| JournalError::Invalid("inode offset exceeds u32".into()))?;
        let journal_source_offset = payload_source_offset
            .checked_add(u64::from(inode_offset_within_payload))
            .ok_or_else(|| JournalError::Invalid("journal inode offset overflows".into()))?;
        let declared_size = inode_declared_size(inode)?;
        let inode_checksum = verify_inode_checksum(filesystem, inode_number, inode)?;
        let content_mapping = match (kind, inode_checksum) {
            (DeletedInodeKind::RegularFile, Some(false)) => DeletedContentMapping::unavailable(
                &JournalError::Invalid("recovered inode checksum mismatch".into()),
            ),
            (DeletedInodeKind::RegularFile, _) => {
                map_deleted_inode_content(filesystem, inode_number, inode, declared_size)
                    .unwrap_or_else(|error| DeletedContentMapping::unavailable(&error))
            }
            (DeletedInodeKind::Directory | DeletedInodeKind::SymbolicLink, _) => {
                DeletedContentMapping::metadata_only()
            }
        };
        candidates.push(DeletedInodeCandidate {
            inode: inode_number,
            kind,
            mode,
            declared_size,
            deletion_time,
            transaction_sequence: mapping.transaction_sequence,
            descriptor_journal_block: mapping.descriptor_journal_block,
            payload_journal_block: mapping.payload_journal_block,
            inode_offset_within_payload,
            journal_source_offset,
            journal_source_length: u32::from(filesystem.inode_size),
            inode_table_group: layout.group,
            inode_table_block: mapping.target_filesystem_block,
            tag_marked_deleted: mapping.flags & JBD2_FLAG_DELETED != 0,
            replay_revoked: mapping.revoked,
            journal_checksum_verified: checksum_verified,
            inode_checksum_verified: inode_checksum == Some(true),
            completeness: content_mapping.completeness(declared_size),
            recoverable_bytes: content_mapping.recoverable_bytes,
            content_mapping,
            confidence: match (checksum_verified, mapping.revoked) {
                (true, false) => 0.9,
                (true, true) => 0.85,
                (false, false) => 0.75,
                (false, true) => 0.7,
            },
            recovery_method: "jbd2_inode_table_snapshot".to_string(),
        });
    }
    Ok(())
}

fn inode_table_layouts(filesystem: &Ext4Reader) -> JournalResult<Vec<InodeTableLayout>> {
    let table_bytes = u64::from(filesystem.inodes_per_group)
        .checked_mul(u64::from(filesystem.inode_size))
        .ok_or_else(|| JournalError::Invalid("inode-table byte length overflows".into()))?;
    let table_blocks = table_bytes.div_ceil(filesystem.block_size);
    let mut layouts = Vec::new();
    for group in 0..filesystem.num_block_groups {
        let first_inode = u64::from(group) * u64::from(filesystem.inodes_per_group);
        if first_inode >= u64::from(filesystem.inodes_count) {
            break;
        }
        let descriptor = filesystem.read_bg_descriptor(group)?;
        let start_block = inode_table_block_from_descriptor(&descriptor, filesystem.has_64bit)?;
        let end_block = start_block
            .checked_add(table_blocks)
            .ok_or_else(|| JournalError::Invalid("inode-table block range overflows".into()))?;
        let remaining = u64::from(filesystem.inodes_count) - first_inode;
        layouts.push(InodeTableLayout {
            group,
            start_block,
            end_block,
            inode_count: remaining.min(u64::from(filesystem.inodes_per_group)) as u32,
        });
    }
    layouts.sort_by_key(|layout| layout.start_block);
    for pair in layouts.windows(2) {
        if pair[0].end_block > pair[1].start_block {
            return Err(JournalError::Invalid(format!(
                "inode-table ranges for groups {} and {} overlap",
                pair[0].group, pair[1].group
            )));
        }
    }
    Ok(layouts)
}

fn find_inode_table_layout(
    layouts: &[InodeTableLayout],
    target_block: u64,
) -> Option<&InodeTableLayout> {
    let index = layouts.partition_point(|layout| layout.start_block <= target_block);
    index
        .checked_sub(1)
        .and_then(|index| layouts.get(index))
        .filter(|layout| target_block < layout.end_block)
}

fn unescaped_payload(payload: &[u8], flags: u32) -> Cow<'_, [u8]> {
    if flags & JBD2_FLAG_ESCAPE == 0 || payload.len() < 4 {
        return Cow::Borrowed(payload);
    }
    let mut restored = payload.to_vec();
    restored[..4].copy_from_slice(&JBD2_MAGIC_NUMBER.to_be_bytes());
    Cow::Owned(restored)
}

fn deleted_inode_metadata(inode: &[u8]) -> Option<(DeletedInodeKind, u16, u32)> {
    if inode.len() < MIN_EXT4_INODE_SIZE {
        return None;
    }
    let mode = u16::from_le_bytes([inode[0], inode[1]]);
    let kind = match mode & EXT4_S_IFMT {
        EXT4_S_IFREG => DeletedInodeKind::RegularFile,
        EXT4_S_IFDIR => DeletedInodeKind::Directory,
        EXT4_S_IFLNK => DeletedInodeKind::SymbolicLink,
        _ => return None,
    };
    let links_count = u16::from_le_bytes([inode[0x1A], inode[0x1B]]);
    let deletion_time = u32::from_le_bytes([inode[0x14], inode[0x15], inode[0x16], inode[0x17]]);
    (links_count == 0 && deletion_time != 0).then_some((kind, mode, deletion_time))
}

fn inode_declared_size(inode: &[u8]) -> JournalResult<u64> {
    let low = u64::from(u32::from_le_bytes(inode[0x04..0x08].try_into().map_err(
        |_| JournalError::Invalid("inode size field is truncated".into()),
    )?));
    let high = u64::from(u32::from_le_bytes(inode[0x6C..0x70].try_into().map_err(
        |_| JournalError::Invalid("inode high-size field is truncated".into()),
    )?));
    Ok(low | (high << 32))
}

#[derive(Debug)]
struct InodeTableLayout {
    group: u32,
    start_block: u64,
    end_block: u64,
    inode_count: u32,
}
