use super::{XfsLogError, XfsLogFormat, XFS_LI_INODE};

const XFS_ILOG_CORE: u32 = 0x0001;
const XFS_DINODE_MAGIC: u16 = 0x494E;
const XFS_LOG_DINODE_V2_SIZE: usize = 96;
const XFS_LOG_DINODE_V3_SIZE: usize = 176;
const XFS_INODE_LOG_FORMAT_32_SIZE: usize = 52;
const XFS_INODE_LOG_FORMAT_SIZE: usize = 56;
const XFS_LOG_ITEM_MIN_REGIONS: u16 = 2;
const XFS_INODE_LOG_ITEM_MAX_REGIONS: u16 = 4;
const XFS_LOG_DINODE_NLINK_OFFSET: usize = 16;
const XFS_LOG_DINODE_V1_ONLINK_OFFSET: usize = 6;
const XFS_LOG_DINODE_V3_INO_OFFSET: usize = 152;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct XfsInodeLogFormat {
    pub inode: u64,
    pub disk_block: i64,
    pub fields: u32,
    pub region_count: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct XfsLoggedInodeCore {
    pub version: u8,
    pub link_count: u32,
    pub inode: Option<u64>,
}

pub(crate) fn parse_inode_log_format(
    format: XfsLogFormat,
    region: &[u8],
) -> Result<XfsInodeLogFormat, XfsLogError> {
    let (inode_offset, block_offset, length_offset, buffer_offset) = match region.len() {
        XFS_INODE_LOG_FORMAT_SIZE => (16, 40, 48, 52),
        XFS_INODE_LOG_FORMAT_32_SIZE => (12, 36, 44, 48),
        length => {
            return Err(invalid(format!(
                "inode log format has {length} bytes; expected 52 or 56"
            )))
        }
    };
    if native_u16(format, region, 0)? != XFS_LI_INODE {
        return Err(invalid("log item is not an XFS inode descriptor"));
    }
    let region_count = native_u16(format, region, 2)?;
    if !(XFS_LOG_ITEM_MIN_REGIONS..=XFS_INODE_LOG_ITEM_MAX_REGIONS).contains(&region_count) {
        return Err(invalid(format!(
            "inode log item declares invalid region count {region_count}"
        )));
    }
    let fields = native_u32(format, region, 4)?;
    if fields & XFS_ILOG_CORE == 0 {
        return Err(invalid("inode log item does not include XFS_ILOG_CORE"));
    }
    let inode = native_u64(format, region, inode_offset)?;
    let disk_block = native_i64(format, region, block_offset)?;
    let buffer_length = native_u32(format, region, length_offset)? as i32;
    let inode_buffer_offset = native_u32(format, region, buffer_offset)? as i32;
    if inode == 0 {
        return Err(invalid("inode log format has a zero inode identity"));
    }
    if disk_block < 0 || buffer_length <= 0 || inode_buffer_offset < 0 {
        return Err(invalid(
            "inode log format has invalid inode-buffer geometry",
        ));
    }
    Ok(XfsInodeLogFormat {
        inode,
        disk_block,
        fields,
        region_count,
    })
}

pub(crate) fn parse_logged_inode_core(
    format: XfsLogFormat,
    region: &[u8],
) -> Result<XfsLoggedInodeCore, XfsLogError> {
    let version = *region
        .get(4)
        .ok_or_else(|| invalid("logged inode core is truncated before di_version"))?;
    let required_size = match version {
        1 | 2 => XFS_LOG_DINODE_V2_SIZE,
        3 => XFS_LOG_DINODE_V3_SIZE,
        _ => {
            return Err(invalid(format!(
                "logged inode core has unsupported version {version}"
            )))
        }
    };
    if region.len() != required_size {
        return Err(invalid(format!(
            "logged inode core version {version} has {} bytes; expected {required_size}",
            region.len()
        )));
    }
    if native_u16(format, region, 0)? != XFS_DINODE_MAGIC {
        return Err(invalid("logged inode core has invalid XFS inode magic"));
    }
    // Linux v2.6.12's xfs_dinode_core places the v1 link count in di_onlink.
    // xfs_inode_item_format copies di_nlink there before logging and explicitly
    // treats the newer fields as untrusted while the inode remains version 1.
    let link_count = if version == 1 {
        u32::from(native_u16(format, region, XFS_LOG_DINODE_V1_ONLINK_OFFSET)?)
    } else {
        native_u32(format, region, XFS_LOG_DINODE_NLINK_OFFSET)?
    };
    Ok(XfsLoggedInodeCore {
        version,
        link_count,
        inode: (version == 3)
            .then(|| native_u64(format, region, XFS_LOG_DINODE_V3_INO_OFFSET))
            .transpose()?,
    })
}

fn native_u16(format: XfsLogFormat, bytes: &[u8], offset: usize) -> Result<u16, XfsLogError> {
    format
        .native_u16(bytes, offset)
        .ok_or_else(|| invalid(format!("cannot decode native-endian u16 at byte {offset}")))
}

fn native_u32(format: XfsLogFormat, bytes: &[u8], offset: usize) -> Result<u32, XfsLogError> {
    format
        .native_u32(bytes, offset)
        .ok_or_else(|| invalid(format!("cannot decode native-endian u32 at byte {offset}")))
}

fn native_u64(format: XfsLogFormat, bytes: &[u8], offset: usize) -> Result<u64, XfsLogError> {
    format
        .native_u64(bytes, offset)
        .ok_or_else(|| invalid(format!("cannot decode native-endian u64 at byte {offset}")))
}

fn native_i64(format: XfsLogFormat, bytes: &[u8], offset: usize) -> Result<i64, XfsLogError> {
    format
        .native_i64(bytes, offset)
        .ok_or_else(|| invalid(format!("cannot decode native-endian i64 at byte {offset}")))
}

fn invalid(message: impl Into<String>) -> XfsLogError {
    XfsLogError::InvalidData(message.into())
}
