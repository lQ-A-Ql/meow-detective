use super::{
    XfsDirectoryEntry, XFS_DIR2_BLOCK_MAGIC, XFS_DIR2_BLOCK_MAGIC_LEGACY, XFS_DIR2_DATA_ALIGN,
    XFS_DIR2_DATA_ENTRY_FIXED_SIZE, XFS_DIR2_DATA_ENTRY_TAG_SIZE, XFS_DIR2_DATA_HDR_SIZE,
    XFS_DIR2_DATA_MAGIC, XFS_DIR2_DATA_MAGIC_LEGACY, XFS_DIR2_FREE_TAG, XFS_DIR3_BLOCK_MAGIC,
    XFS_DIR3_DATA_ENTRY_FTYPE_SIZE, XFS_DIR3_DATA_HDR_SIZE, XFS_DIR3_DATA_MAGIC, XFS_DIR3_FT_MAX,
    XFS_DIR3_FT_UNKNOWN,
};
use crate::{be_u16, be_u32, be_u64, XfsReader};
use evidence_core::filesystem::invalid_fs_data;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryBlockKind {
    Block { header_size: usize, dir3: bool },
    Data { header_size: usize, dir3: bool },
    Zero,
    Unknown(u32),
}

impl DirectoryBlockKind {
    fn prefers_ftype(self) -> bool {
        matches!(
            self,
            Self::Block { dir3: true, .. } | Self::Data { dir3: true, .. }
        )
    }
}

#[derive(Default)]
pub(super) struct DirectoryBlockParse {
    pub(super) entries: Vec<XfsDirectoryEntry>,
    pub(super) error: Option<io::Error>,
    pub(super) saw_recoverable_block: bool,
}

impl XfsReader {
    pub(super) fn parse_block_dir_entries_lossy(&self, data: &[u8]) -> DirectoryBlockParse {
        Self::parse_block_dir_entries_impl(data, true, self.has_ftype)
    }

    pub(super) fn parse_block_dir_entries_impl(
        data: &[u8],
        recoverable_magic: bool,
        has_ftype: bool,
    ) -> DirectoryBlockParse {
        if data.len() < 8 {
            return if data.is_empty() {
                DirectoryBlockParse::default()
            } else {
                DirectoryBlockParse {
                    error: Some(invalid_fs_data("block directory buffer too short")),
                    ..DirectoryBlockParse::default()
                }
            };
        }

        let block_kind = Self::classify_directory_block(data);
        let (header_size, has_block_tail, preferred_has_ftype) = match block_kind {
            DirectoryBlockKind::Block { header_size, .. } => {
                (header_size, true, has_ftype || block_kind.prefers_ftype())
            }
            DirectoryBlockKind::Data { header_size, .. } => {
                (header_size, false, has_ftype || block_kind.prefers_ftype())
            }
            DirectoryBlockKind::Zero => {
                return DirectoryBlockParse {
                    error: (!recoverable_magic)
                        .then(|| invalid_fs_data("zeroed block directory data")),
                    saw_recoverable_block: true,
                    ..DirectoryBlockParse::default()
                };
            }
            DirectoryBlockKind::Unknown(magic) => {
                return DirectoryBlockParse {
                    error: Some(invalid_fs_data(format!(
                        "unknown block directory magic 0x{magic:08X}"
                    ))),
                    saw_recoverable_block: recoverable_magic,
                    ..DirectoryBlockParse::default()
                };
            }
        };
        if data.len() <= header_size {
            return DirectoryBlockParse::default();
        }

        let data_end = if has_block_tail && data.len() >= 8 {
            let leaf_count = be_u32(data, data.len() - 8) as usize;
            data.len()
                .saturating_sub(8)
                .saturating_sub(leaf_count * 8)
                .max(header_size)
        } else {
            data.len()
        };

        let entries = parse_directory_entries(data, header_size, data_end, preferred_has_ftype);
        DirectoryBlockParse {
            entries,
            ..DirectoryBlockParse::default()
        }
    }

    fn classify_directory_block(data: &[u8]) -> DirectoryBlockKind {
        if data.len() < 4 || data.iter().all(|byte| *byte == 0) {
            return DirectoryBlockKind::Zero;
        }
        match be_u32(data, 0) {
            XFS_DIR3_BLOCK_MAGIC => DirectoryBlockKind::Block {
                header_size: XFS_DIR3_DATA_HDR_SIZE,
                dir3: true,
            },
            XFS_DIR2_BLOCK_MAGIC | XFS_DIR2_BLOCK_MAGIC_LEGACY => DirectoryBlockKind::Block {
                header_size: XFS_DIR2_DATA_HDR_SIZE,
                dir3: false,
            },
            XFS_DIR3_DATA_MAGIC => DirectoryBlockKind::Data {
                header_size: XFS_DIR3_DATA_HDR_SIZE,
                dir3: true,
            },
            XFS_DIR2_DATA_MAGIC | XFS_DIR2_DATA_MAGIC_LEGACY => DirectoryBlockKind::Data {
                header_size: XFS_DIR2_DATA_HDR_SIZE,
                dir3: false,
            },
            magic => DirectoryBlockKind::Unknown(magic),
        }
    }
}

fn parse_directory_entries(
    data: &[u8],
    header_size: usize,
    data_end: usize,
    preferred_has_ftype: bool,
) -> Vec<XfsDirectoryEntry> {
    let mut position = header_size;
    let mut entries = Vec::new();
    while position + 11 <= data_end {
        if !position.is_multiple_of(XFS_DIR2_DATA_ALIGN) {
            break;
        }
        let free_tag = be_u16(data, position);
        if free_tag == XFS_DIR2_FREE_TAG {
            if position + 4 > data.len() {
                break;
            }
            let skip_len = usize::from(be_u16(data, position + 2));
            if !valid_unused_dir_record(data, position, skip_len, data_end) {
                break;
            }
            position = position.saturating_add(skip_len);
            continue;
        }

        let inode = be_u64(data, position);
        let name_len = usize::from(data[position + 8]);
        if inode == 0 && name_len == 0 {
            break;
        }
        let name_start = position + 9;
        if name_len == 0 {
            position = position.saturating_add(16);
            continue;
        }
        let name_end = name_start + name_len;
        if name_end > data_end {
            break;
        }
        let Some((file_type, padded_end)) =
            decode_dir_entry_tail(data, name_end, position, data_end, preferred_has_ftype)
        else {
            break;
        };
        let name = String::from_utf8_lossy(&data[name_start..name_end]).to_string();
        if padded_end <= position {
            break;
        }
        position = padded_end;

        if inode != 0
            && is_plausible_directory_entry_name(&name)
            && !entries
                .iter()
                .any(|entry: &XfsDirectoryEntry| entry.name == name)
        {
            entries.push(XfsDirectoryEntry {
                name,
                inode,
                ftype: file_type,
            });
        }
    }
    entries
}

fn decode_dir_entry_tail(
    data: &[u8],
    name_end: usize,
    entry_start: usize,
    data_end: usize,
    preferred_has_ftype: bool,
) -> Option<(Option<u8>, usize)> {
    decode_dir_entry_tail_with_layout(data, name_end, entry_start, data_end, preferred_has_ftype)
        .or_else(|| {
            decode_dir_entry_tail_with_layout(
                data,
                name_end,
                entry_start,
                data_end,
                !preferred_has_ftype,
            )
        })
}

fn decode_dir_entry_tail_with_layout(
    data: &[u8],
    name_end: usize,
    entry_start: usize,
    data_end: usize,
    has_ftype: bool,
) -> Option<(Option<u8>, usize)> {
    if !entry_start.is_multiple_of(XFS_DIR2_DATA_ALIGN) || name_end < entry_start {
        return None;
    }
    let name_start = entry_start.checked_add(XFS_DIR2_DATA_ENTRY_FIXED_SIZE)?;
    let name_len = name_end.checked_sub(name_start)?;
    let record_len = dir_entry_record_size(name_len, has_ftype)?;
    let padded_end = entry_start.checked_add(record_len)?;
    if padded_end > data_end || record_len < XFS_DIR2_DATA_ENTRY_TAG_SIZE {
        return None;
    }
    let tag_position = padded_end.checked_sub(XFS_DIR2_DATA_ENTRY_TAG_SIZE)?;
    if usize::from(be_u16(data, tag_position)) != entry_start {
        return None;
    }
    let file_type = if has_ftype {
        let value = *data.get(name_end)?;
        if value >= XFS_DIR3_FT_MAX {
            return None;
        }
        (value != XFS_DIR3_FT_UNKNOWN).then_some(value)
    } else {
        None
    };
    Some((file_type, padded_end))
}

fn dir_entry_record_size(name_len: usize, has_ftype: bool) -> Option<usize> {
    let file_type_size = usize::from(has_ftype) * XFS_DIR3_DATA_ENTRY_FTYPE_SIZE;
    XFS_DIR2_DATA_ENTRY_FIXED_SIZE
        .checked_add(name_len)?
        .checked_add(file_type_size)?
        .checked_add(XFS_DIR2_DATA_ENTRY_TAG_SIZE)
        .and_then(|length| align_up(length, XFS_DIR2_DATA_ALIGN))
}

fn valid_unused_dir_record(
    data: &[u8],
    entry_start: usize,
    record_len: usize,
    data_end: usize,
) -> bool {
    if record_len < XFS_DIR2_DATA_ALIGN || !record_len.is_multiple_of(XFS_DIR2_DATA_ALIGN) {
        return false;
    }
    let Some(record_end) = entry_start.checked_add(record_len) else {
        return false;
    };
    record_end <= data_end
        && record_end >= 2
        && usize::from(be_u16(data, record_end - 2)) == entry_start
}

fn align_up(value: usize, align: usize) -> Option<usize> {
    if align == 0 {
        return Some(value);
    }
    let add = align.checked_sub(1)?;
    value.checked_add(add).map(|result| result & !add)
}

/// Structural-only plausibility check for the primary parse path. XFS
/// directory names are arbitrary non-NUL byte strings (commonly UTF-8), so a
/// character whitelist here would silently drop valid evidence; the raw
/// recovery heuristics apply their own stricter name rules.
fn is_plausible_directory_entry_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 255 && !name.contains('\0') && !matches!(name, "." | "..")
}
