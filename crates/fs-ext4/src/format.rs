use evidence_core::filesystem::invalid_fs_data;
use std::io;

pub(crate) const EXT4_SUPERBLOCK_OFFSET: u64 = 1024;
pub(crate) const EXT4_MAGIC: u16 = 0xEF53;
pub(crate) const EXT4_EXTENT_MAGIC: u16 = 0xF30A;
pub(crate) const EXT4_FEATURE_INCOMPAT_64BIT: u32 = 0x0080;
pub(crate) const EXT4_MIN_GROUP_DESCRIPTOR_SIZE: u16 = 32;
pub(crate) const EXT4_64BIT_GROUP_DESCRIPTOR_SIZE: u16 = 64;
pub(crate) const EXT4_METADATA_CACHE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const S_IFDIR: u16 = 0x4000;
pub(crate) const S_IFLNK: u16 = 0xA000;
pub(crate) const I_BLOCK_SIZE: usize = 60;

#[derive(Debug)]
pub(crate) struct Ext4ExtentHeader {
    pub(crate) eh_entries: u16,
    pub(crate) eh_depth: u16,
}

impl Ext4ExtentHeader {
    pub(crate) fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < 12 {
            return Err(invalid_fs_data("extent header too short"));
        }
        let magic = u16::from_le_bytes([data[0], data[1]]);
        if magic != EXT4_EXTENT_MAGIC {
            return Err(invalid_fs_data(format!(
                "invalid extent header magic 0x{:04X}",
                magic
            )));
        }
        Ok(Self {
            eh_entries: u16::from_le_bytes([data[2], data[3]]),
            eh_depth: u16::from_le_bytes([data[6], data[7]]),
        })
    }
}

#[derive(Debug)]
pub(crate) struct Ext4Extent {
    pub(crate) ee_block: u32,
    pub(crate) ee_len: u16,
    pub(crate) ee_start_hi: u16,
    pub(crate) ee_start_lo: u32,
}

impl Ext4Extent {
    pub(crate) fn parse(data: &[u8]) -> io::Result<Self> {
        Ok(Self {
            ee_block: u32::from_le_bytes(
                data[0..4]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            ),
            ee_len: u16::from_le_bytes([data[4], data[5]]),
            ee_start_hi: u16::from_le_bytes([data[6], data[7]]),
            ee_start_lo: u32::from_le_bytes(
                data[8..12]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            ),
        })
    }

    pub(crate) fn block_count(&self) -> u16 {
        self.ee_len & 0x7FFF
    }

    pub(crate) fn is_unwritten(&self) -> bool {
        self.ee_len & 0x8000 != 0
    }
}

pub(crate) fn inode_table_block_from_descriptor(
    descriptor: &[u8],
    has_64bit: bool,
) -> io::Result<u64> {
    let low = descriptor
        .get(0x08..0x0C)
        .ok_or_else(|| invalid_fs_data("ext4 group descriptor is missing inode table block"))?;
    let low = u32::from_le_bytes(
        low.try_into()
            .map_err(|_| invalid_fs_data("disk parse error"))?,
    ) as u64;
    let high = if has_64bit {
        let high = descriptor.get(0x28..0x2C).ok_or_else(|| {
            invalid_fs_data("64-bit ext4 group descriptor is missing inode table high bits")
        })?;
        u32::from_le_bytes(
            high.try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        ) as u64
    } else {
        0
    };
    let block = low | (high << u32::BITS);
    if block == 0 {
        return Err(invalid_fs_data("ext4 inode table block is zero"));
    }
    Ok(block)
}
