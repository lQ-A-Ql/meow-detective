use super::record::{be_u16, be_u32, be_u64};
use super::{
    collect_log_records, parse_log_entries, RecoveredFile, XfsLogEntry, XLOG_DEFAULT_BLOCK_SIZE,
    XLOG_ITEM_BUF, XLOG_ITEM_INODE,
};
use std::io;

#[derive(Debug, Clone)]
pub(super) struct LoggedInodeCore {
    ino: u64,
    _mode: u16,
    size: u64,
    nextents: u32,
    format: u8,
    pub(super) is_deleted: bool,
}

pub fn recover_metadata_operations(log_data: &[u8]) -> io::Result<Vec<XfsLogEntry>> {
    let records = collect_log_records(log_data, XLOG_DEFAULT_BLOCK_SIZE)?;
    let mut entries = Vec::new();
    for (_, payload) in records {
        entries.extend(parse_log_entries(&payload)?);
    }
    Ok(entries)
}

pub fn recover_deleted_inodes(log_data: &[u8]) -> io::Result<Vec<RecoveredFile>> {
    let entries = recover_metadata_operations(log_data)?;
    let inode_entries = entries
        .iter()
        .filter(|entry| entry.item_type == XLOG_ITEM_INODE)
        .collect::<Vec<_>>();
    let buffer_data = entries
        .iter()
        .filter(|entry| entry.item_type == XLOG_ITEM_BUF)
        .map(|entry| entry.data.clone())
        .collect::<Vec<_>>();

    let mut recovered = Vec::new();
    for entry in inode_entries {
        if entry.target_ino == 0 {
            continue;
        }
        let Some(inode_core) = parse_logged_inode_core(&entry.data) else {
            continue;
        };
        if !inode_core.is_deleted {
            continue;
        }
        recovered.push(RecoveredFile {
            original_path: format!(
                "$OrphanInode{}/log_recovered_inode_{}",
                entry.target_ino, entry.target_ino
            ),
            inode: inode_core.ino,
            blocks: buffer_data.clone(),
            declared_size: inode_core.size,
            recovery_method: format!("xlog_inode_item_format_{}", inode_core.format),
            confidence: compute_log_confidence(&inode_core, buffer_data.len() as u64),
            block_count: buffer_data.len() as u64,
        });
    }

    for buffer in &buffer_data {
        let Some((name, inode)) = extract_dirent_from_buf(buffer) else {
            continue;
        };
        if recovered.iter().any(|file| file.inode == inode) {
            continue;
        }
        recovered.push(RecoveredFile {
            original_path: format!("$OrphanInode{inode}/dirent_hint_{name}"),
            inode,
            blocks: vec![buffer.clone()],
            declared_size: buffer.len() as u64,
            recovery_method: "xlog_dirent_hint".to_string(),
            confidence: 0.25,
            block_count: 1,
        });
    }
    Ok(recovered)
}

pub(super) fn parse_logged_inode_core(data: &[u8]) -> Option<LoggedInodeCore> {
    if data.len() < 2 {
        return None;
    }
    if be_u16(data, 0) == 0x494E {
        return Some(parse_inode_fields(data, 0));
    }
    for base in [4, 8] {
        if data.len() >= base + 2 && be_u16(data, base) == 0x494E {
            return Some(parse_inode_fields(data, base));
        }
    }
    None
}

fn parse_inode_fields(data: &[u8], base: usize) -> LoggedInodeCore {
    let safe_u8 = |offset| data.get(offset).copied().unwrap_or(0);
    let safe_u16 = |offset| {
        if offset + 2 <= data.len() {
            be_u16(data, offset)
        } else {
            0
        }
    };
    let safe_u32 = |offset| {
        if offset + 4 <= data.len() {
            be_u32(data, offset)
        } else {
            0
        }
    };
    let safe_u64 = |offset| {
        if offset + 8 <= data.len() {
            be_u64(data, offset)
        } else {
            0
        }
    };

    let nlink = if base + 0x64 <= data.len() {
        safe_u32(base + 0x60)
    } else if base + 0x12 <= data.len() {
        u32::from(safe_u16(base + 0x10))
    } else {
        0
    };
    LoggedInodeCore {
        ino: if base >= 8 { safe_u64(base - 8) } else { 0 },
        _mode: safe_u16(base + 2),
        size: safe_u64(base + 0x38),
        nextents: safe_u32(base + 0x4C),
        format: safe_u8(base + 5),
        is_deleted: nlink == 0,
    }
}

fn compute_log_confidence(inode: &LoggedInodeCore, buffer_count: u64) -> f64 {
    let mut confidence: f64 = 0.25;
    if inode.size > 0 {
        confidence += 0.15;
    }
    if inode.nextents > 0 {
        confidence += 0.10;
    }
    if buffer_count > 0 {
        confidence += 0.25;
        let expected = inode.size.div_ceil(4096);
        if expected > 0 && buffer_count >= expected {
            confidence += 0.25;
        }
    }
    confidence.min(1.0)
}

pub(super) fn extract_dirent_from_buf(buf: &[u8]) -> Option<(String, u64)> {
    let mut offset = 0usize;
    while offset + 9 < buf.len() {
        let name_len = usize::from(buf[offset]);
        if name_len == 0 || name_len > 255 {
            offset += 1;
            continue;
        }
        let name_start = offset + 1;
        let name_end = name_start + name_len;
        if name_end + 8 > buf.len() {
            offset += 1;
            continue;
        }
        let name = String::from_utf8_lossy(&buf[name_start..name_end]);
        if name.is_empty()
            || name
                .chars()
                .any(|character| character.is_control() && character != '\0')
        {
            offset += 1;
            continue;
        }
        let inode = be_u64(buf, name_end);
        if inode > 0 {
            return Some((name.to_string(), inode));
        }
        offset = name_end + 8;
    }
    None
}
