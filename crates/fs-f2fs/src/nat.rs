use std::collections::HashMap;

use crate::checkpoint::Checkpoint;
use crate::io::{block_offset, read_exact_at, read_u32, SharedReader};
use crate::{F2fsError, F2fsSuperblock, Result, F2FS_BLOCK_SIZE};

const NAT_ENTRY_SIZE: u32 = 9;
const NAT_ENTRIES_PER_BLOCK: u32 = F2FS_BLOCK_SIZE as u32 / NAT_ENTRY_SIZE;

#[derive(Debug, Clone, Copy)]
pub(crate) struct NatEntry {
    pub(crate) inode: u32,
    pub(crate) block: u32,
}

pub(crate) struct NatTable {
    nat_block: u32,
    blocks_per_segment: u32,
    bitmap: Vec<u8>,
    journal: HashMap<u32, NatEntry>,
}

impl NatTable {
    pub(crate) fn new(superblock: &F2fsSuperblock, checkpoint: &Checkpoint) -> Self {
        let journal = checkpoint
            .nat_journal
            .iter()
            .map(|(nid, entry)| {
                (
                    *nid,
                    NatEntry {
                        inode: entry.inode,
                        block: entry.block,
                    },
                )
            })
            .collect();
        Self {
            nat_block: superblock.nat_block,
            blocks_per_segment: superblock.blocks_per_segment,
            bitmap: checkpoint.nat_bitmap.clone(),
            journal,
        }
    }

    pub(crate) fn lookup(
        &self,
        source: &SharedReader,
        volume_offset: u64,
        nid: u32,
    ) -> Result<NatEntry> {
        if let Some(entry) = self.journal.get(&nid).copied() {
            if entry.block == 0 {
                return Err(F2fsError::NotFound(format!("inode {nid}")));
            }
            return Ok(entry);
        }
        let block_index = nid / NAT_ENTRIES_PER_BLOCK;
        let entry_index = nid % NAT_ENTRIES_PER_BLOCK;
        let mut block = self.current_block(block_index)?;
        if block >= u32::MAX - 1 {
            return Err(F2fsError::Invalid(
                "NAT block address overflows".to_string(),
            ));
        }
        let bytes = read_exact_at(source, block_offset(volume_offset, block)?, F2FS_BLOCK_SIZE)?;
        let offset = entry_index as usize * NAT_ENTRY_SIZE as usize;
        let inode = read_u32(&bytes, offset + 1, "NAT inode")?;
        block = read_u32(&bytes, offset + 5, "NAT block address")?;
        if block == 0 {
            return Err(F2fsError::NotFound(format!("inode {nid}")));
        }
        Ok(NatEntry { inode, block })
    }

    fn current_block(&self, block_index: u32) -> Result<u32> {
        let segment_offset = block_index / self.blocks_per_segment;
        let offset_in_segment = block_index % self.blocks_per_segment;
        let paired_offset = segment_offset
            .checked_mul(self.blocks_per_segment)
            .and_then(|value| value.checked_mul(2))
            .and_then(|value| value.checked_add(offset_in_segment))
            .ok_or_else(|| F2fsError::Invalid("NAT address overflows".to_string()))?;
        let selected = if bitmap_bit(&self.bitmap, block_index)? {
            self.blocks_per_segment
        } else {
            0
        };
        self.nat_block
            .checked_add(paired_offset)
            .and_then(|value| value.checked_add(selected))
            .ok_or_else(|| F2fsError::Invalid("NAT copy address overflows".to_string()))
    }
}

fn bitmap_bit(bitmap: &[u8], index: u32) -> Result<bool> {
    let byte = bitmap
        .get(index as usize / 8)
        .ok_or_else(|| F2fsError::Invalid(format!("NAT bitmap does not cover block {index}")))?;
    Ok(byte & (1 << (7 - (index % 8))) != 0)
}
