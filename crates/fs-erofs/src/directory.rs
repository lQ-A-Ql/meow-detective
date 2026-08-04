use crate::io::{read_u16, read_u64};
use crate::{ErofsError, Result, EROFS_BLOCK_SIZE};

const DIRENT_SIZE: usize = 12;
const METABOX_NID_BIT: u64 = 1u64 << 63;

#[derive(Debug, Clone)]
pub(crate) struct DirectoryEntry {
    pub(crate) nid: u64,
    pub(crate) file_type: u8,
    pub(crate) name: String,
}

pub(crate) fn parse_directory_block(
    bytes: &[u8],
    valid_length: usize,
) -> Result<Vec<DirectoryEntry>> {
    let bytes = bytes
        .get(..valid_length.min(EROFS_BLOCK_SIZE))
        .ok_or_else(|| ErofsError::Invalid("directory block length is invalid".to_string()))?;
    if bytes.is_empty() || bytes.iter().all(|byte| *byte == 0) {
        return Ok(Vec::new());
    }
    if bytes.len() < DIRENT_SIZE {
        return Err(ErofsError::Invalid(
            "directory block is shorter than one dirent".to_string(),
        ));
    }
    let first_name_offset = usize::from(read_u16(bytes, 8, "directory name offset")?);
    if first_name_offset < DIRENT_SIZE || first_name_offset % DIRENT_SIZE != 0 {
        return Err(ErofsError::Invalid(format!(
            "invalid first directory name offset {first_name_offset}"
        )));
    }
    if first_name_offset > bytes.len() {
        return Err(ErofsError::Invalid(
            "directory entry table exceeds its block".to_string(),
        ));
    }
    let entry_count = first_name_offset / DIRENT_SIZE;
    let mut entries = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        let offset = index * DIRENT_SIZE;
        let nid = read_u64(bytes, offset, "directory nid")?;
        if nid & METABOX_NID_BIT != 0 {
            return Err(ErofsError::Unsupported(
                "metabox directory inode".to_string(),
            ));
        }
        let name_start = usize::from(read_u16(bytes, offset + 8, "directory name offset")?);
        let name_end = if index + 1 < entry_count {
            usize::from(read_u16(
                bytes,
                (index + 1) * DIRENT_SIZE + 8,
                "next directory name offset",
            )?)
        } else {
            bytes.len()
        };
        if name_start < first_name_offset || name_start >= name_end || name_end > bytes.len() {
            return Err(ErofsError::Invalid(format!(
                "invalid directory name range {name_start}..{name_end}"
            )));
        }
        let mut name_bytes = &bytes[name_start..name_end];
        while name_bytes.last() == Some(&0) {
            name_bytes = &name_bytes[..name_bytes.len() - 1];
        }
        let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| {
            ErofsError::Invalid(format!("directory entry {index} is not valid UTF-8"))
        })?;
        if name.is_empty() {
            return Err(ErofsError::Invalid(format!(
                "directory entry {index} has an empty name"
            )));
        }
        if name != "." && name != ".." {
            entries.push(DirectoryEntry {
                nid,
                file_type: bytes[offset + 10],
                name,
            });
        }
    }
    Ok(entries)
}
