use crate::{
    di_off, XfsReader, FORMAT_BTREE, FORMAT_EXTENTS, FORMAT_LOCAL, INODE_CORE_SIZE,
    INODE_CORE_SIZE_V3,
};
use evidence_core::filesystem::{
    invalid_fs_data, FileSystemDiagnostic, FileSystemDiagnosticKind, FsTimestamp,
};
use std::io;

const NSEC_PER_SEC: u64 = 1_000_000_000;
const XFS_DIFLAG2_BIGTIME: u64 = 1 << 3;
const XFS_BIGTIME_EPOCH_OFFSET: i64 = 2_147_483_648;
const XFS_BIGTIME_TIME_MAX: u64 = (u64::MAX / NSEC_PER_SEC) & !3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XfsInodeMetadata {
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
    pub(crate) created_at: Option<FsTimestamp>,
    pub(crate) modified_at: Option<FsTimestamp>,
    pub(crate) accessed_at: Option<FsTimestamp>,
    pub(crate) changed_at: Option<FsTimestamp>,
}

impl XfsReader {
    pub(crate) fn inode_metadata(&self, ino: u64) -> io::Result<XfsInodeMetadata> {
        let inode = self.read_inode(ino)?;
        self.decode_inode_metadata_with_diagnostics(ino, &inode)
    }

    pub(crate) fn decode_inode_metadata(inode: &[u8]) -> io::Result<XfsInodeMetadata> {
        Self::validate_inode_magic(inode)?;
        let version = *inode
            .get(di_off::VERSION)
            .ok_or_else(|| invalid_fs_data("inode truncated before version"))?;
        let required_size = match version {
            1 | 2 => INODE_CORE_SIZE,
            3 => INODE_CORE_SIZE_V3,
            other => {
                return Err(invalid_fs_data(format!(
                    "unsupported XFS inode version {other}"
                )))
            }
        };
        if inode.len() < required_size {
            return Err(invalid_fs_data(format!(
                "XFS v{version} inode core truncated: have {} bytes, need {required_size}",
                inode.len()
            )));
        }

        let bigtime = version == 3
            && read_be_u64(inode, di_off::FLAGS2, "di_flags2")? & XFS_DIFLAG2_BIGTIME != 0;
        let accessed_at = decode_timestamp(inode, di_off::ATIME, "di_atime", bigtime)?;
        let modified_at = decode_timestamp(inode, di_off::MTIME, "di_mtime", bigtime)?;
        let changed_at = decode_timestamp(inode, di_off::CTIME, "di_ctime", bigtime)?;
        let created_at = if version == 3 {
            Some(decode_timestamp(
                inode,
                di_off::CRTIME,
                "di_crtime",
                bigtime,
            )?)
        } else {
            None
        };

        Ok(XfsInodeMetadata {
            is_dir: Self::inode_is_dir(inode),
            size: read_be_u64(inode, di_off::SIZE, "di_size")?,
            created_at,
            modified_at: Some(modified_at),
            accessed_at: Some(accessed_at),
            changed_at: Some(changed_at),
        })
    }

    pub(crate) fn decode_inode_metadata_with_diagnostics(
        &self,
        ino: u64,
        inode: &[u8],
    ) -> io::Result<XfsInodeMetadata> {
        match Self::decode_inode_metadata(inode) {
            Ok(metadata) => Ok(metadata),
            Err(timestamp_error) => {
                let metadata = Self::decode_inode_structural_metadata(inode)?;
                self.record_diagnostic(
                    FileSystemDiagnostic::new(
                        FileSystemDiagnosticKind::MetadataDegraded,
                        format!(
                            "XFS inode {ino} retained without one or more timestamps: {timestamp_error}"
                        ),
                    )
                    .with_inode(ino),
                );
                Ok(metadata)
            }
        }
    }

    pub(crate) fn validate_directory_inode_metadata(
        ino: u64,
        inode: &[u8],
        metadata: &XfsInodeMetadata,
    ) -> io::Result<()> {
        if metadata.is_dir
            && !matches!(
                inode[di_off::FORMAT],
                FORMAT_LOCAL | FORMAT_EXTENTS | FORMAT_BTREE
            )
        {
            return Err(invalid_fs_data(format!(
                "directory inode {ino} uses unsupported format {}",
                inode[di_off::FORMAT]
            )));
        }
        Ok(())
    }

    fn decode_inode_structural_metadata(inode: &[u8]) -> io::Result<XfsInodeMetadata> {
        Self::validate_inode_magic(inode)?;
        let version = *inode
            .get(di_off::VERSION)
            .ok_or_else(|| invalid_fs_data("inode truncated before version"))?;
        let required_size = match version {
            1 | 2 => INODE_CORE_SIZE,
            3 => INODE_CORE_SIZE_V3,
            other => {
                return Err(invalid_fs_data(format!(
                    "unsupported XFS inode version {other}"
                )))
            }
        };
        if inode.len() < required_size {
            return Err(invalid_fs_data(format!(
                "XFS v{version} inode core truncated: have {} bytes, need {required_size}",
                inode.len()
            )));
        }
        Ok(XfsInodeMetadata {
            is_dir: Self::inode_is_dir(inode),
            size: read_be_u64(inode, di_off::SIZE, "di_size")?,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
        })
    }
}

fn decode_timestamp(
    inode: &[u8],
    offset: usize,
    field: &str,
    bigtime: bool,
) -> io::Result<FsTimestamp> {
    if bigtime {
        decode_bigtime(read_be_u64(inode, offset, field)?, field)
    } else {
        let seconds = i64::from(read_be_i32(inode, offset, field)?);
        let nanoseconds = read_be_u32(inode, offset + 4, field)?;
        datetime_from_parts(seconds, nanoseconds, field)
    }
}

fn decode_bigtime(total_nanoseconds: u64, field: &str) -> io::Result<FsTimestamp> {
    let seconds = total_nanoseconds / NSEC_PER_SEC;
    if seconds > XFS_BIGTIME_TIME_MAX {
        return Err(invalid_fs_data(format!(
            "{field} exceeds supported XFS BIGTIME range"
        )));
    }
    let unix_seconds = i64::try_from(seconds)
        .ok()
        .and_then(|value| value.checked_sub(XFS_BIGTIME_EPOCH_OFFSET))
        .ok_or_else(|| invalid_fs_data(format!("{field} BIGTIME epoch conversion overflows")))?;
    let nanoseconds = (total_nanoseconds % NSEC_PER_SEC) as u32;
    datetime_from_parts(unix_seconds, nanoseconds, field)
}

fn datetime_from_parts(seconds: i64, nanoseconds: u32, field: &str) -> io::Result<FsTimestamp> {
    if u64::from(nanoseconds) >= NSEC_PER_SEC {
        return Err(invalid_fs_data(format!(
            "{field} has invalid nanoseconds {nanoseconds}"
        )));
    }
    FsTimestamp::from_timestamp(seconds, nanoseconds).ok_or_else(|| {
        invalid_fs_data(format!(
            "{field} timestamp is outside the supported datetime range"
        ))
    })
}

fn read_be_i32(inode: &[u8], offset: usize, field: &str) -> io::Result<i32> {
    let bytes = field_bytes(inode, offset, 4, field)?;
    Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_be_u32(inode: &[u8], offset: usize, field: &str) -> io::Result<u32> {
    let bytes = field_bytes(inode, offset, 4, field)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_be_u64(inode: &[u8], offset: usize, field: &str) -> io::Result<u64> {
    let bytes = field_bytes(inode, offset, 8, field)?;
    Ok(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn field_bytes<'a>(
    inode: &'a [u8],
    offset: usize,
    length: usize,
    field: &str,
) -> io::Result<&'a [u8]> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| invalid_fs_data(format!("{field} offset overflows")))?;
    inode.get(offset..end).ok_or_else(|| {
        invalid_fs_data(format!(
            "inode truncated while reading {field} at 0x{offset:X}"
        ))
    })
}

#[cfg(test)]
#[path = "../tests/unit/timestamps.rs"]
mod tests;
