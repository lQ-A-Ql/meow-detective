use std::path::Path;

use crate::{x64::translate_raw, MemoryWindowsError, RawMemoryImage, Result};

const LOW_MEMORY_LIMIT: usize = 1024 * 1024;
const START_BLOCK_ALIGNMENT: usize = 0x1000;
const START_BLOCK_SIGNATURE: u64 = 0x0000_0001_0006_00E9;
const START_BLOCK_SIGNATURE_MASK: u64 = 0xFFFF_FFFF_FFFF_00FF;
const LONG_MODE_TARGET_OFFSET: usize = 0x70;
const DIRECTORY_TABLE_BASE_OFFSET: usize = 0xA0;
const START_BLOCK_REQUIRED_LEN: usize = DIRECTORY_TABLE_BASE_OFFSET + 8;
const KERNEL_ADDRESS_MASK: u64 = 0xFFFF_F800_0000_0003;
const KERNEL_ADDRESS_PREFIX: u64 = 0xFFFF_F800_0000_0000;
const CR3_INVALID_BITS: u64 = 0xFFFF_FF00_0000_0FFF;
const MAX_START_BLOCKS: usize = 64;

/// The non-secret paging bootstrap values retained in low physical memory by x64 Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessorStartBlock {
    pub physical_address: u64,
    pub long_mode_target: u64,
    pub directory_table_base: u64,
}

/// Reads only the first MiB and returns structurally valid processor start blocks.
pub fn find_processor_start_blocks(image: &mut RawMemoryImage) -> Result<Vec<ProcessorStartBlock>> {
    let read_len = image.len().min(LOW_MEMORY_LIMIT as u64) as usize;
    if read_len < START_BLOCK_REQUIRED_LEN {
        return Err(MemoryWindowsError::ProcessorStartBlockNotFound);
    }
    let mut low_memory = vec![0u8; read_len];
    image.read_exact_at(0, &mut low_memory)?;

    let mut blocks = Vec::new();
    for offset in (START_BLOCK_ALIGNMENT..read_len).step_by(START_BLOCK_ALIGNMENT) {
        let Some(bytes) = low_memory.get(offset..offset + START_BLOCK_REQUIRED_LEN) else {
            break;
        };
        let signature = read_u64(bytes, 0);
        let long_mode_target = read_u64(bytes, LONG_MODE_TARGET_OFFSET);
        let directory_table_base = read_u64(bytes, DIRECTORY_TABLE_BASE_OFFSET);
        if signature & START_BLOCK_SIGNATURE_MASK != START_BLOCK_SIGNATURE
            || long_mode_target & KERNEL_ADDRESS_MASK != KERNEL_ADDRESS_PREFIX
            || directory_table_base & CR3_INVALID_BITS != 0
            || directory_table_base == 0
            || directory_table_base
                .checked_add(0x1000)
                .is_none_or(|end| end > image.len())
        {
            continue;
        }
        if translate_raw(image, directory_table_base, long_mode_target).is_err() {
            continue;
        }
        blocks.push(ProcessorStartBlock {
            physical_address: offset as u64,
            long_mode_target,
            directory_table_base,
        });
        if blocks.len() == MAX_START_BLOCKS {
            break;
        }
    }
    if blocks.is_empty() {
        return Err(MemoryWindowsError::ProcessorStartBlockNotFound);
    }
    Ok(blocks)
}

/// Recovers an independently validated x64 CR3 without KDBG or symbol files.
pub fn discover_directory_table_base(path: &Path) -> Result<ProcessorStartBlock> {
    let mut image = RawMemoryImage::open(path)?;
    find_processor_start_blocks(&mut image)?
        .into_iter()
        .next()
        .ok_or(MemoryWindowsError::ProcessorStartBlockNotFound)
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated start-block range"),
    )
}
