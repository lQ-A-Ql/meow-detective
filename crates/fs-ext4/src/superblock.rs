use crate::format::{
    EXT4_64BIT_GROUP_DESCRIPTOR_SIZE, EXT4_FEATURE_COMPAT_HAS_JOURNAL, EXT4_FEATURE_INCOMPAT_64BIT,
    EXT4_FEATURE_INCOMPAT_CSUM_SEED, EXT4_FEATURE_RO_COMPAT_BIGALLOC,
    EXT4_FEATURE_RO_COMPAT_GDT_CSUM, EXT4_FEATURE_RO_COMPAT_METADATA_CSUM, EXT4_MAGIC,
    EXT4_MIN_GROUP_DESCRIPTOR_SIZE, EXT4_SUPERBLOCK_OFFSET,
};
use evidence_core::filesystem::invalid_fs_data;
use evidence_core::EvidenceReader;
use std::io::{self, SeekFrom};

pub(crate) struct Ext4Superblock {
    pub(crate) block_size: u64,
    pub(crate) blocks_count: u64,
    pub(crate) blocks_per_group: u32,
    pub(crate) first_data_block: u64,
    pub(crate) inode_size: u16,
    pub(crate) inodes_per_group: u32,
    pub(crate) inodes_count: u32,
    pub(crate) filesystem_uuid: [u8; 16],
    pub(crate) group_descriptor_size: u16,
    pub(crate) has_64bit: bool,
    pub(crate) has_bigalloc: bool,
    pub(crate) has_gdt_csum: bool,
    pub(crate) has_metadata_csum: bool,
    pub(crate) checksum_seed: u32,
    pub(crate) num_block_groups: u32,
    pub(crate) has_journal: bool,
    pub(crate) journal_inode: Option<u32>,
}

impl Ext4Superblock {
    pub(crate) fn read(reader: &mut dyn EvidenceReader, offset: u64) -> io::Result<Self> {
        let superblock_offset = offset
            .checked_add(EXT4_SUPERBLOCK_OFFSET)
            .ok_or_else(|| invalid_fs_data("ext4 superblock offset overflows"))?;
        reader.seek(SeekFrom::Start(superblock_offset))?;
        let mut data = [0u8; 1024];
        reader.read_exact(&mut data)?;
        validate_magic(&data)?;

        let log_block_size = read_u32(&data, 0x18)?;
        if log_block_size > 6 {
            return Err(invalid_fs_data(format!(
                "unsupported ext4 log block size {log_block_size}"
            )));
        }
        let block_size = 1024u64
            .checked_shl(log_block_size)
            .ok_or_else(|| invalid_fs_data("ext4 block size overflows"))?;
        let inodes_count = read_u32(&data, 0x00)?;
        let blocks_count_lo = read_u32(&data, 0x04)?;
        let blocks_per_group = read_u32(&data, 0x20)?;
        let inodes_per_group = read_u32(&data, 0x28)?;
        let first_data_block = u64::from(read_u32(&data, 0x14)?);
        let raw_inode_size = u16::from_le_bytes([data[0x58], data[0x59]]);
        let inode_size = if raw_inode_size == 0 {
            128
        } else {
            raw_inode_size
        };
        let feature_compat = read_u32(&data, 0x5C)?;
        let feature_incompat = read_u32(&data, 0x60)?;
        let feature_ro_compat = read_u32(&data, 0x64)?;
        let mut filesystem_uuid = [0u8; 16];
        filesystem_uuid.copy_from_slice(&data[0x68..0x78]);
        let has_journal = feature_compat & EXT4_FEATURE_COMPAT_HAS_JOURNAL != 0;
        let raw_journal_inode = read_u32(&data, 0xE0)?;
        let journal_inode = (has_journal && raw_journal_inode != 0).then_some(raw_journal_inode);
        let has_64bit = feature_incompat & EXT4_FEATURE_INCOMPAT_64BIT != 0;
        let has_bigalloc = feature_ro_compat & EXT4_FEATURE_RO_COMPAT_BIGALLOC != 0;
        let has_gdt_csum = feature_ro_compat & EXT4_FEATURE_RO_COMPAT_GDT_CSUM != 0;
        let has_metadata_csum = feature_ro_compat & EXT4_FEATURE_RO_COMPAT_METADATA_CSUM != 0;
        let checksum_seed = checksum_seed(&data, feature_incompat, &filesystem_uuid)?;
        let blocks_count_hi = if has_64bit {
            read_u32(&data, 0x150)?
        } else {
            0
        };
        let blocks_count = u64::from(blocks_count_lo) | (u64::from(blocks_count_hi) << u32::BITS);
        let raw_descriptor_size = u16::from_le_bytes([data[0xFE], data[0xFF]]);
        let group_descriptor_size = if has_64bit {
            raw_descriptor_size
        } else {
            EXT4_MIN_GROUP_DESCRIPTOR_SIZE
        };
        validate_geometry(
            block_size,
            blocks_count,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            has_64bit,
            group_descriptor_size,
        )?;
        let num_block_groups = block_group_count(
            blocks_count,
            first_data_block,
            blocks_per_group,
            inodes_count,
            inodes_per_group,
        )?;

        Ok(Self {
            block_size,
            blocks_count,
            blocks_per_group,
            first_data_block,
            inode_size,
            inodes_per_group,
            inodes_count,
            filesystem_uuid,
            group_descriptor_size,
            has_64bit,
            has_bigalloc,
            has_gdt_csum,
            has_metadata_csum,
            checksum_seed,
            num_block_groups,
            has_journal,
            journal_inode,
        })
    }
}

fn validate_magic(data: &[u8]) -> io::Result<()> {
    let magic = u16::from_le_bytes([data[0x38], data[0x39]]);
    if magic != EXT4_MAGIC {
        return Err(invalid_fs_data(format!(
            "not a valid ext4 filesystem (magic 0x{magic:04X})"
        )));
    }
    Ok(())
}

fn checksum_seed(
    data: &[u8],
    feature_incompat: u32,
    filesystem_uuid: &[u8; 16],
) -> io::Result<u32> {
    if feature_incompat & EXT4_FEATURE_INCOMPAT_CSUM_SEED != 0 {
        read_u32(data, 0x270)
    } else {
        Ok(crate::journal::checksum::crc32c(u32::MAX, filesystem_uuid))
    }
}

fn block_group_count(
    blocks_count: u64,
    first_data_block: u64,
    blocks_per_group: u32,
    inodes_count: u32,
    inodes_per_group: u32,
) -> io::Result<u32> {
    let block_groups = blocks_count
        .saturating_sub(first_data_block)
        .div_ceil(u64::from(blocks_per_group));
    let inode_groups = u64::from(inodes_count).div_ceil(u64::from(inodes_per_group));
    u32::try_from(block_groups.max(inode_groups))
        .map_err(|_| invalid_fs_data("ext4 block group count exceeds u32"))
}

fn read_u32(data: &[u8], offset: usize) -> io::Result<u32> {
    Ok(u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .map_err(|_| invalid_fs_data("disk parse error"))?,
    ))
}

fn validate_geometry(
    block_size: u64,
    blocks_count: u64,
    blocks_per_group: u32,
    inodes_per_group: u32,
    inode_size: u16,
    has_64bit: bool,
    descriptor_size: u16,
) -> io::Result<()> {
    if blocks_count == 0 || blocks_per_group == 0 || inodes_per_group == 0 {
        return Err(invalid_fs_data("invalid ext4 geometry"));
    }
    if inode_size < 128 || !inode_size.is_power_of_two() || u64::from(inode_size) > block_size {
        return Err(invalid_fs_data(format!(
            "invalid ext4 inode size {inode_size} for block size {block_size}"
        )));
    }
    if has_64bit && descriptor_size < EXT4_64BIT_GROUP_DESCRIPTOR_SIZE {
        return Err(invalid_fs_data(format!(
            "64-bit ext4 group descriptor size {descriptor_size} is smaller than {EXT4_64BIT_GROUP_DESCRIPTOR_SIZE} bytes"
        )));
    }
    if descriptor_size < EXT4_MIN_GROUP_DESCRIPTOR_SIZE
        || u64::from(descriptor_size) > block_size
        || !block_size.is_multiple_of(u64::from(descriptor_size))
    {
        return Err(invalid_fs_data(format!(
            "invalid ext4 group descriptor size {descriptor_size} for block size {block_size}"
        )));
    }
    Ok(())
}
