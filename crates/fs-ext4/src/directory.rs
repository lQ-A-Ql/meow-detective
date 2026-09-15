use crate::format::{
    require_extents_layout, EXT4_INLINE_DATA_FL, EXT4_XATTR_ENTRY_HEADER_SIZE,
    EXT4_XATTR_INDEX_SYSTEM, EXT4_XATTR_MAGIC, I_BLOCK_SIZE, S_IFDIR,
};
use crate::Ext4Reader;
use evidence_core::filesystem::{invalid_fs_data, path_components};
use std::io;

impl Ext4Reader {
    fn parse_directory_entries(data: &[u8]) -> io::Result<Vec<(String, u32, u8)>> {
        let mut entries = Vec::new();
        let mut offset = 0usize;
        while offset + 8 <= data.len() {
            let inode = u32::from_le_bytes(
                data[offset..offset + 4]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            let record_length = u16::from_le_bytes([data[offset + 4], data[offset + 5]]) as usize;
            let name_length = data[offset + 6] as usize;
            let file_type = data[offset + 7];
            if record_length < 8
                || !record_length.is_multiple_of(4)
                || offset + record_length > data.len()
            {
                break;
            }
            if inode == 0 {
                offset += record_length;
                continue;
            }
            if name_length > 0 && name_length <= record_length - 8 {
                let start = offset + 8;
                let end = (start..start + name_length)
                    .find(|&index| data[index] == 0)
                    .unwrap_or(start + name_length);
                let name = String::from_utf8_lossy(&data[start..end]).to_string();
                if !name.is_empty() {
                    entries.push((name, inode, file_type));
                }
            }
            offset += record_length;
        }
        Ok(entries)
    }

    pub(crate) fn read_directory_entries(
        &self,
        inode_number: u32,
    ) -> io::Result<Vec<(String, u32, u8)>> {
        let inode = self.read_inode(inode_number)?;
        if Self::inode_mode(&inode)? & S_IFDIR == 0 {
            return Err(invalid_fs_data(format!(
                "inode {} is not a directory",
                inode_number
            )));
        }
        if Self::inode_is_encrypted(&inode)? {
            return Err(evidence_core::filesystem::unsupported_fs(format!(
                "directory inode {} is encrypted",
                inode_number
            )));
        }
        require_extents_layout(&inode, &format!("directory inode {inode_number}"))?;
        let data = self.read_extent_data(Self::inode_i_block(&inode), Self::inode_size(&inode)?)?;
        Self::parse_directory_entries(&data)
    }

    pub(crate) fn resolve_path(&self, path: &str) -> io::Result<Option<(u32, bool)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((2, true)));
        }
        let mut current_inode = 2;
        for (index, component) in components.iter().enumerate() {
            let entries = self.read_directory_entries(current_inode)?;
            let is_last = index == components.len() - 1;
            match entries.iter().find(|(name, _, _)| name == component) {
                Some((_, inode_number, file_type)) => {
                    let is_dir = *file_type == 2;
                    if is_last {
                        return Ok(Some((*inode_number, is_dir)));
                    }
                    if !is_dir {
                        return Ok(None);
                    }
                    current_inode = *inode_number;
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }

    pub(crate) fn read_symlink_target(&self, inode: &[u8]) -> io::Result<String> {
        let size = Self::inode_size(inode)? as usize;
        if Self::inode_flags(inode)? & EXT4_INLINE_DATA_FL != 0 {
            let value = inline_data_value(Self::inode_i_block(inode))?;
            return Ok(decode_target(&value[..size.min(value.len())]));
        }
        if size < I_BLOCK_SIZE {
            let block = Self::inode_i_block(inode);
            let bytes = &block[..size.min(block.len())];
            Ok(decode_target(bytes))
        } else {
            let data = self.read_extent_data(Self::inode_i_block(inode), size as u64)?;
            Ok(String::from_utf8_lossy(&data).to_string())
        }
    }
}

fn decode_target(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

/// Locates the "system.data" value holding an inline-data inode's payload
/// inside `i_block` (xattr ibody layout, see ext4 inline_data.rst). Values
/// spilled into an external xattr block are not reachable here and fail
/// closed as invalid.
fn inline_data_value(i_block: &[u8]) -> io::Result<&[u8]> {
    let magic = i_block
        .get(0..4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| invalid_fs_data("inline data header out of bounds"))?;
    if magic != EXT4_XATTR_MAGIC {
        return Err(invalid_fs_data(format!(
            "invalid inline data xattr magic 0x{magic:08X}"
        )));
    }
    let mut offset = 4usize;
    while offset + EXT4_XATTR_ENTRY_HEADER_SIZE <= i_block.len() {
        let entry = &i_block[offset..offset + EXT4_XATTR_ENTRY_HEADER_SIZE];
        let name_len = usize::from(entry[0]);
        let name_index = entry[1];
        let value_offset = usize::from(u16::from_le_bytes([entry[2], entry[3]]));
        let value_inum = u32::from_le_bytes(
            entry[4..8]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let value_size = u32::from_le_bytes(
            entry[8..12]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        ) as usize;
        if name_len == 0 && name_index == 0 {
            break;
        }
        let name_start = offset + EXT4_XATTR_ENTRY_HEADER_SIZE;
        let name_end = name_start + name_len;
        if name_end > i_block.len() {
            return Err(invalid_fs_data("inline data xattr name exceeds i_block"));
        }
        if name_index == EXT4_XATTR_INDEX_SYSTEM && i_block[name_start..name_end] == *b"data" {
            if value_inum != 0 {
                return Err(evidence_core::filesystem::unsupported_fs(
                    "inline data value is stored in another inode",
                ));
            }
            let value_end = value_offset
                .checked_add(value_size)
                .ok_or_else(|| invalid_fs_data("inline data value range overflows"))?;
            if value_end > i_block.len() {
                return Err(invalid_fs_data("inline data value exceeds i_block"));
            }
            return Ok(&i_block[value_offset..value_end]);
        }
        offset = name_end
            .checked_add(3)
            .map(|end| end & !3)
            .ok_or_else(|| invalid_fs_data("inline data xattr entry overflows"))?;
    }
    Err(invalid_fs_data(
        "inline data inode has no system.data value",
    ))
}
