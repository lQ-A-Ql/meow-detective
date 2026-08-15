use crate::format::{DIR_INDEX_KEY, DIR_ITEM_KEY, FT_DIR, INODE_ITEM_KEY};
use crate::types::BtrfsKey;
use crate::BtrfsReader;
use evidence_core::filesystem::{invalid_fs_data, path_components, FsTimestamp};
use std::io;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BtrfsInodeMetadata {
    pub(crate) size: u64,
    pub(crate) read_only: bool,
    pub(crate) unix_mode: u32,
    pub(crate) created_at: Option<FsTimestamp>,
    pub(crate) modified_at: Option<FsTimestamp>,
    pub(crate) accessed_at: Option<FsTimestamp>,
    pub(crate) changed_at: Option<FsTimestamp>,
}

impl BtrfsReader {
    fn parse_dir_entry(data: &[u8]) -> io::Result<Option<(String, u64, u8)>> {
        if data.len() < 30 {
            return Ok(None);
        }
        let child_obj = u64::from_le_bytes(
            data[0..8]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let name_len = u16::from_le_bytes(
            data[27..29]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        ) as usize;
        let file_type = data[29];
        if data.len() < 30 + name_len {
            return Ok(None);
        }
        let name = String::from_utf8_lossy(&data[30..30 + name_len]).to_string();
        if name.is_empty() {
            return Ok(None);
        }
        Ok(Some((name, child_obj, file_type)))
    }

    pub(crate) fn list_dir_entries(
        &self,
        tree_root_bytenr: u64,
        dir_objectid: u64,
    ) -> io::Result<Vec<(String, u64, u8)>> {
        let search_key = BtrfsKey {
            objectid: dir_objectid,
            ty: DIR_INDEX_KEY,
            offset: 0,
        };
        let (leaf_data, items) = self.walk_to_leaf(tree_root_bytenr, &search_key)?;
        let mut entries =
            Self::entries_for_key_type(&leaf_data, &items, dir_objectid, DIR_INDEX_KEY)?;
        if entries.is_empty() {
            entries = Self::entries_for_key_type(&leaf_data, &items, dir_objectid, DIR_ITEM_KEY)?;
        }
        Ok(entries)
    }

    fn entries_for_key_type(
        leaf_data: &[u8],
        items: &[crate::types::LeafItem],
        dir_objectid: u64,
        key_type: u8,
    ) -> io::Result<Vec<(String, u64, u8)>> {
        let mut entries = Vec::new();
        for index in Self::find_items_by_object_and_type(items, dir_objectid, key_type) {
            if let Some(entry) =
                Self::parse_dir_entry(Self::get_item_data(leaf_data, &items[index]))?
            {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    pub(crate) fn get_inode_size(
        &self,
        tree_root_bytenr: u64,
        inode_objectid: u64,
    ) -> io::Result<u64> {
        Ok(self
            .get_inode_metadata(tree_root_bytenr, inode_objectid)?
            .map(|metadata| metadata.size)
            .unwrap_or_default())
    }

    pub(crate) fn get_inode_metadata(
        &self,
        tree_root_bytenr: u64,
        inode_objectid: u64,
    ) -> io::Result<Option<BtrfsInodeMetadata>> {
        let key = BtrfsKey {
            objectid: inode_objectid,
            ty: INODE_ITEM_KEY,
            offset: 0,
        };
        let (leaf_data, items) = self.walk_to_leaf(tree_root_bytenr, &key)?;
        if let Ok(index) = items.binary_search_by(|item| item.key.cmp(&key)) {
            let data = Self::get_item_data(&leaf_data, &items[index]);
            if data.len() >= 160 {
                let size = u64::from_le_bytes(
                    data[16..24]
                        .try_into()
                        .map_err(|_| invalid_fs_data("disk parse error"))?,
                );
                let mode = u32::from_le_bytes(
                    data[52..56]
                        .try_into()
                        .map_err(|_| invalid_fs_data("disk parse error"))?,
                );
                return Ok(Some(BtrfsInodeMetadata {
                    size,
                    read_only: mode & 0o222 == 0,
                    unix_mode: mode,
                    created_at: parse_inode_timestamp(data, 148),
                    modified_at: parse_inode_timestamp(data, 136),
                    accessed_at: parse_inode_timestamp(data, 112),
                    changed_at: parse_inode_timestamp(data, 124),
                }));
            }
        }
        Ok(None)
    }

    pub(crate) fn resolve_path_in_tree(
        &self,
        tree_root_bytenr: u64,
        root_dirid: u64,
        path: &str,
    ) -> io::Result<Option<(u64, bool, u64)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((root_dirid, true, 0)));
        }

        let mut current_dir = root_dirid;
        for (index, component) in components.iter().enumerate() {
            let entries = self.list_dir_entries(tree_root_bytenr, current_dir)?;
            let is_last = index == components.len() - 1;
            match entries.iter().find(|(name, _, _)| name == component) {
                Some((_, inode_objectid, file_type)) => {
                    let is_dir = *file_type == FT_DIR;
                    if is_last {
                        let size = self.get_inode_size(tree_root_bytenr, *inode_objectid)?;
                        return Ok(Some((*inode_objectid, is_dir, size)));
                    }
                    if !is_dir {
                        return Ok(None);
                    }
                    current_dir = *inode_objectid;
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }
}

fn parse_inode_timestamp(data: &[u8], offset: usize) -> Option<FsTimestamp> {
    let seconds = i64::from_le_bytes(data.get(offset..offset.checked_add(8)?)?.try_into().ok()?);
    let nanoseconds = u32::from_le_bytes(
        data.get(offset.checked_add(8)?..offset.checked_add(12)?)?
            .try_into()
            .ok()?,
    );
    FsTimestamp::from_timestamp(seconds, nanoseconds)
}
