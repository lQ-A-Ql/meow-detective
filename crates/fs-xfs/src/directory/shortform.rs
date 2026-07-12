use super::{DIR2_SF_HDR_4, DIR2_SF_HDR_8, XFS_DIR3_FT_MAX};
use crate::{be_u32, be_u64, XfsReader};
use evidence_core::filesystem::invalid_fs_data;
use std::io;

impl XfsReader {
    pub(super) fn parse_shortform_dir(
        data_fork: &[u8],
        has_ftype: bool,
    ) -> io::Result<Vec<(String, u64)>> {
        if data_fork.len() < DIR2_SF_HDR_4 {
            return Err(invalid_fs_data("shortform dir too small for header"));
        }
        let count = usize::from(data_fork[0]);
        let i8count = usize::from(data_fork[1]);
        let header_size = if i8count == 0 {
            DIR2_SF_HDR_4
        } else {
            DIR2_SF_HDR_8
        };
        if data_fork.len() < header_size {
            return Err(invalid_fs_data("shortform dir header truncated"));
        }

        let mut position = header_size;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let Some((name, inode, next_position)) =
                parse_shortform_entry(data_fork, position, i8count != 0, has_ftype)
            else {
                break;
            };
            entries.push((name, inode));
            position = next_position;
        }
        Ok(entries)
    }
}

fn parse_shortform_entry(
    data: &[u8],
    position: usize,
    uses_64_bit_inode: bool,
    has_ftype: bool,
) -> Option<(String, u64, usize)> {
    if position + 3 > data.len() {
        return None;
    }
    let name_len = usize::from(data[position]);
    let name_start = position + 3;
    let name_end = name_start.checked_add(name_len)?;
    if name_len == 0 || name_end > data.len() {
        return None;
    }
    let (inode, tail_len) = parse_shortform_inode(data, name_end, uses_64_bit_inode, has_ftype)?;
    let name = String::from_utf8_lossy(&data[name_start..name_end]).to_string();
    Some((name, inode, name_end + tail_len))
}

fn parse_shortform_inode(
    data: &[u8],
    name_end: usize,
    uses_64_bit_inode: bool,
    has_ftype: bool,
) -> Option<(u64, usize)> {
    let inode_offset = if has_ftype {
        if *data.get(name_end)? >= XFS_DIR3_FT_MAX {
            return None;
        }
        name_end + 1
    } else {
        name_end
    };
    if uses_64_bit_inode {
        let bytes = data.get(inode_offset..inode_offset + 8)?;
        Some((
            be_u64(bytes, 0) & 0x00FF_FFFF_FFFF_FFFF,
            8 + usize::from(has_ftype),
        ))
    } else {
        let bytes = data.get(inode_offset..inode_offset + 4)?;
        Some((u64::from(be_u32(bytes, 0)), 4 + usize::from(has_ftype)))
    }
}
