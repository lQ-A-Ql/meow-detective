use crate::format::{require_extents_layout, I_BLOCK_SIZE, S_IFDIR};
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
            if record_length < 8 || offset + record_length > data.len() {
                break;
            }
            if name_length > 0 && offset + 8 + name_length <= data.len() {
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
        if size < I_BLOCK_SIZE {
            let block = Self::inode_i_block(inode);
            let bytes = &block[..size.min(block.len())];
            let end = bytes
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(bytes.len());
            Ok(String::from_utf8_lossy(&bytes[..end]).to_string())
        } else {
            let data = self.read_extent_data(Self::inode_i_block(inode), size as u64)?;
            Ok(String::from_utf8_lossy(&data).to_string())
        }
    }
}
