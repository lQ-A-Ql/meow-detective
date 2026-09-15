use evidence_core::filesystem::{invalid_fs_data, unsupported_fs};
use std::io;

pub(crate) const EXT4_SUPERBLOCK_OFFSET: u64 = 1024;
pub(crate) const EXT4_MAGIC: u16 = 0xEF53;
pub(crate) const EXT4_EXTENT_MAGIC: u16 = 0xF30A;
pub(crate) const EXT4_FEATURE_COMPAT_HAS_JOURNAL: u32 = 0x0004;
pub(crate) const EXT4_FEATURE_RO_COMPAT_GDT_CSUM: u32 = 0x0010;
pub(crate) const EXT4_FEATURE_RO_COMPAT_BIGALLOC: u32 = 0x0200;
pub(crate) const EXT4_FEATURE_RO_COMPAT_METADATA_CSUM: u32 = 0x0400;
pub(crate) const EXT4_FEATURE_INCOMPAT_FILETYPE: u32 = 0x0002;
pub(crate) const EXT4_FEATURE_INCOMPAT_RECOVER: u32 = 0x0004;
pub(crate) const EXT4_FEATURE_INCOMPAT_EXTENTS: u32 = 0x0040;
pub(crate) const EXT4_FEATURE_INCOMPAT_64BIT: u32 = 0x0080;
pub(crate) const EXT4_FEATURE_INCOMPAT_MMP: u32 = 0x0100;
pub(crate) const EXT4_FEATURE_INCOMPAT_FLEX_BG: u32 = 0x0200;
pub(crate) const EXT4_FEATURE_INCOMPAT_EA_INODE: u32 = 0x0400;
pub(crate) const EXT4_FEATURE_INCOMPAT_CSUM_SEED: u32 = 0x2000;
pub(crate) const EXT4_FEATURE_INCOMPAT_LARGEDIR: u32 = 0x4000;
pub(crate) const EXT4_FEATURE_INCOMPAT_INLINE_DATA: u32 = 0x8000;
pub(crate) const EXT4_FEATURE_INCOMPAT_ENCRYPT: u32 = 0x0001_0000;
pub(crate) const EXT4_FEATURE_INCOMPAT_CASEFOLD: u32 = 0x0002_0000;
/// Incompat bits this reader either implements or can read past unchanged.
/// Anything else (compression, external journal device, META_BG, DIRDATA,
/// or future bits) changes the on-disk interpretation and must fail closed.
pub(crate) const EXT4_KNOWN_INCOMPAT_FEATURES: u32 = EXT4_FEATURE_INCOMPAT_FILETYPE
    | EXT4_FEATURE_INCOMPAT_RECOVER
    | EXT4_FEATURE_INCOMPAT_EXTENTS
    | EXT4_FEATURE_INCOMPAT_64BIT
    | EXT4_FEATURE_INCOMPAT_MMP
    | EXT4_FEATURE_INCOMPAT_FLEX_BG
    | EXT4_FEATURE_INCOMPAT_EA_INODE
    | EXT4_FEATURE_INCOMPAT_CSUM_SEED
    | EXT4_FEATURE_INCOMPAT_LARGEDIR
    | EXT4_FEATURE_INCOMPAT_INLINE_DATA
    | EXT4_FEATURE_INCOMPAT_ENCRYPT
    | EXT4_FEATURE_INCOMPAT_CASEFOLD;
pub(crate) const EXT4_MIN_GROUP_DESCRIPTOR_SIZE: u16 = 32;
pub(crate) const EXT4_64BIT_GROUP_DESCRIPTOR_SIZE: u16 = 64;
pub(crate) const EXT4_METADATA_CACHE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const S_IFDIR: u16 = 0x4000;
pub(crate) const S_IFLNK: u16 = 0xA000;
pub(crate) const I_BLOCK_SIZE: usize = 60;
pub(crate) const I_FLAGS_OFFSET: usize = 0x20;
pub(crate) const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
pub(crate) const EXT4_ENCRYPT_FL: u32 = 0x0000_0800;
pub(crate) const EXT4_INLINE_DATA_FL: u32 = 0x1000_0000;
/// ext4_xattr_ibody_header::h_magic marking inline data in `i_block`.
pub(crate) const EXT4_XATTR_MAGIC: u32 = 0xEA02_0000;
/// ext4_xattr_entry::e_name_index of the inline-data "data" value.
pub(crate) const EXT4_XATTR_INDEX_SYSTEM: u8 = 7;
pub(crate) const EXT4_XATTR_ENTRY_HEADER_SIZE: usize = 16;
/// On-disk maximum depth of an ext4 extent tree.
pub(crate) const EXT4_EXTENT_MAX_DEPTH: u16 = 5;

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
        let eh_depth = u16::from_le_bytes([data[6], data[7]]);
        if eh_depth > EXT4_EXTENT_MAX_DEPTH {
            return Err(invalid_fs_data(format!(
                "extent tree depth {eh_depth} exceeds on-disk maximum {EXT4_EXTENT_MAX_DEPTH}"
            )));
        }
        Ok(Self {
            eh_entries: u16::from_le_bytes([data[2], data[3]]),
            eh_depth,
        })
    }
}

/// Refuses to interpret `i_block` as an extent tree unless the inode declares
/// the extents layout and does not carry inline data.
pub(crate) fn require_extents_layout(inode: &[u8], subject: &str) -> io::Result<()> {
    let flags = inode
        .get(I_FLAGS_OFFSET..I_FLAGS_OFFSET + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| invalid_fs_data("inode field out of bounds"))?;
    if flags & EXT4_INLINE_DATA_FL != 0 {
        return Err(unsupported_fs(format!(
            "{subject} uses inline data; extent reads are unsupported"
        )));
    }
    if flags & EXT4_EXTENTS_FL == 0 {
        return Err(invalid_fs_data(format!(
            "{subject} does not use extents; refusing to parse i_block as extents"
        )));
    }
    Ok(())
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

    pub(crate) fn block_count(&self) -> u32 {
        match self.ee_len {
            0 => 0,
            0x8000 => 32_768,
            value if value > 0x8000 => u32::from(value - 0x8000),
            value => u32::from(value),
        }
    }

    pub(crate) fn is_unwritten(&self) -> bool {
        self.ee_len > 0x8000
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
