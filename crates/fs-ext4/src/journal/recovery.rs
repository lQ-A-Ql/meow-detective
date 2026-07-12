use super::{collect_descriptor_blocks, DescriptorBlock, RecoveredFile};
use std::io;

pub(crate) const TAG_FLAG_DELETED: u32 = 4;

pub fn recover_deleted_inodes(
    _fs: &crate::Ext4Reader,
    journal_data: &[u8],
    block_size: usize,
) -> io::Result<Vec<RecoveredFile>> {
    let descriptors = collect_descriptor_blocks(journal_data, block_size)?;
    let mut recovered = Vec::new();
    for descriptor in &descriptors {
        recover_descriptor(descriptor, block_size, &mut recovered);
    }
    Ok(recovered)
}

fn recover_descriptor(
    descriptor: &DescriptorBlock,
    block_size: usize,
    recovered: &mut Vec<RecoveredFile>,
) {
    for (tag_index, tag) in descriptor.tags.iter().enumerate() {
        let is_inode_related =
            tag.flags & TAG_FLAG_DELETED != 0 || is_likely_inode_block(tag.block_number);
        if !is_inode_related || tag_index >= descriptor.block_data.len() {
            continue;
        }
        let block = &descriptor.block_data[tag_index];
        for inode_offset in 0..block_size / 128 {
            let offset = inode_offset * 128;
            if offset + 128 > block.len() {
                break;
            }
            let inode = &block[offset..offset + 128];
            if !is_plausible_deleted_inode(inode) {
                continue;
            }
            let inode_number = tag.block_number * (block_size as u32 / 128) + inode_offset as u32;
            let declared_size = u32::from_le_bytes([inode[4], inode[5], inode[6], inode[7]]) as u64;
            let data_blocks = descriptor
                .tags
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != tag_index)
                .filter_map(|(index, _)| descriptor.block_data.get(index).cloned())
                .collect::<Vec<_>>();
            let confidence = compute_confidence(inode, data_blocks.len() as u64);
            recovered.push(RecoveredFile {
                original_path: format!(
                    "$OrphanInode{}/journal_recovered_inode_{}",
                    inode_number, inode_number
                ),
                inode: inode_number,
                blocks: data_blocks.clone(),
                declared_size,
                recovery_method: "journal_descriptor".to_string(),
                confidence,
                block_count: data_blocks.len() as u64,
            });

            if let Some(data) =
                extract_data_from_i_block(&inode[0x28..0x28 + 60], &descriptor.block_data)
            {
                recovered.push(RecoveredFile {
                    original_path: format!(
                        "$OrphanInode{}/journal_recovered_inode_{}_iblock",
                        inode_number, inode_number
                    ),
                    inode: inode_number,
                    blocks: vec![data],
                    declared_size,
                    recovery_method: "inode_replay".to_string(),
                    confidence: (confidence + 0.1).min(1.0),
                    block_count: 1,
                });
            }
        }
    }
}

fn is_likely_inode_block(_block: u32) -> bool {
    true
}

pub(crate) fn is_plausible_deleted_inode(inode: &[u8]) -> bool {
    if inode.len() < 128 {
        return false;
    }
    let mode = u16::from_le_bytes([inode[0], inode[1]]);
    let links_count = u16::from_le_bytes([inode[0x1A], inode[0x1B]]);
    let size_lo = u32::from_le_bytes([inode[4], inode[5], inode[6], inode[7]]);
    let deletion_time = u32::from_le_bytes([inode[0x14], inode[0x15], inode[0x16], inode[0x17]]);
    mode != 0 && links_count == 0 && (size_lo != 0 || deletion_time != 0)
}

pub(crate) fn compute_confidence(inode: &[u8], data_blocks: u64) -> f64 {
    let size = u32::from_le_bytes([inode[4], inode[5], inode[6], inode[7]]) as u64;
    let deletion_time = u32::from_le_bytes([inode[0x14], inode[0x15], inode[0x16], inode[0x17]]);
    let mut confidence: f64 = 0.3;
    if size > 0 {
        confidence += 0.15;
    }
    if deletion_time > 0 {
        confidence += 0.15;
    }
    if data_blocks > 0 {
        confidence += 0.2;
        if data_blocks >= size.div_ceil(4096) {
            confidence += 0.2;
        }
    }
    confidence.min(1.0)
}

fn extract_data_from_i_block(i_block: &[u8], block_data: &[Vec<u8>]) -> Option<Vec<u8>> {
    for offset in (0..48).step_by(4) {
        if offset + 4 > i_block.len() {
            break;
        }
        let pointer = u32::from_le_bytes([
            i_block[offset],
            i_block[offset + 1],
            i_block[offset + 2],
            i_block[offset + 3],
        ]);
        if pointer == 0 {
            continue;
        }
        if let Some(data) = block_data.iter().find(|data| has_plausible_content(data)) {
            return Some(data.clone());
        }
    }
    None
}

fn has_plausible_content(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    let non_null = data.iter().filter(|&&byte| byte != 0).count();
    non_null > 8 && non_null < data.len().saturating_sub(8)
}
