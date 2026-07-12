use crate::format::{
    BTRFS_HEADER_SIZE, CHUNK_ITEM_KEY, INTERNAL_ITEM_SIZE, KEY_SIZE, LEAF_ITEM_SIZE,
    ROOT_BACKREF_KEY, ROOT_ITEM_KEY,
};
use crate::types::{BtrfsHeader, BtrfsKey, InternalItem, LeafItem};
use crate::BtrfsReader;
use evidence_core::filesystem::invalid_fs_data;
use std::collections::HashMap;
use std::io;

impl BtrfsReader {
    pub(crate) fn read_chunk_tree(&mut self) -> io::Result<()> {
        let node_data = self.read_logical_block(self.chunk_tree_logical)?;
        let header = Self::parse_header(&node_data)?;
        if header.level == 0 {
            self.read_chunk_leaf(&node_data, header.nritems)?;
        } else {
            for item in Self::parse_internal_items(&node_data, header.nritems)? {
                let child = self.read_logical_block(item.blockptr)?;
                let child_header = Self::parse_header(&child)?;
                if child_header.level == 0 {
                    self.read_chunk_leaf(&child, child_header.nritems)?;
                }
            }
        }
        Ok(())
    }

    fn read_chunk_leaf(&mut self, data: &[u8], nritems: u32) -> io::Result<()> {
        for item in Self::parse_leaf_items(data, nritems)? {
            if item.key.ty == CHUNK_ITEM_KEY {
                let chunk_data = Self::get_item_data(data, &item);
                self.parse_chunks(chunk_data)?;
            }
        }
        Ok(())
    }

    pub(crate) fn discover_subvolumes(&mut self) -> io::Result<()> {
        let root_data = self.read_logical_block(self.root_tree_logical)?;
        let header = Self::parse_header(&root_data)?;
        let mut root_items = HashMap::new();
        let mut root_names = HashMap::new();

        if header.level == 0 {
            Self::scan_root_leaf(&root_data, header.nritems, &mut root_items, &mut root_names)?;
        } else {
            for item in Self::parse_internal_items(&root_data, header.nritems)? {
                let child = self.read_logical_block(item.blockptr)?;
                let child_header = Self::parse_header(&child)?;
                if child_header.level == 0 {
                    Self::scan_root_leaf(
                        &child,
                        child_header.nritems,
                        &mut root_items,
                        &mut root_names,
                    )?;
                } else {
                    for nested in Self::parse_internal_items(&child, child_header.nritems)? {
                        let leaf = self.read_logical_block(nested.blockptr)?;
                        let leaf_header = Self::parse_header(&leaf)?;
                        if leaf_header.level == 0 {
                            Self::scan_root_leaf(
                                &leaf,
                                leaf_header.nritems,
                                &mut root_items,
                                &mut root_names,
                            )?;
                        }
                    }
                }
            }
        }

        for (id, (bytenr, root_dirid)) in root_items {
            let name = root_names
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("subvol_{}", id));
            self.subvolumes.push(crate::types::BtrfsSubvol {
                id,
                name,
                root_dirid,
                tree_root_bytenr: bytenr,
            });
        }
        Ok(())
    }

    pub(crate) fn scan_root_leaf(
        data: &[u8],
        nritems: u32,
        root_items: &mut HashMap<u64, (u64, u64)>,
        root_names: &mut HashMap<u64, String>,
    ) -> io::Result<()> {
        for item in Self::parse_leaf_items(data, nritems)? {
            if item.key.ty == ROOT_ITEM_KEY {
                let root_data = Self::get_item_data(data, &item);
                if root_data.len() >= 184 {
                    let bytenr = u64::from_le_bytes(
                        root_data[176..184]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    );
                    let root_dirid = u64::from_le_bytes(
                        root_data[168..176]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    );
                    root_items.insert(item.key.objectid, (bytenr, root_dirid));
                }
            } else if item.key.ty == ROOT_BACKREF_KEY {
                let backref = Self::get_item_data(data, &item);
                if backref.len() >= 18 {
                    let name_len = u16::from_le_bytes(
                        backref[16..18]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    ) as usize;
                    if backref.len() >= 18 + name_len {
                        root_names.insert(
                            item.key.objectid,
                            String::from_utf8_lossy(&backref[18..18 + name_len]).to_string(),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn parse_header(data: &[u8]) -> io::Result<BtrfsHeader> {
        if data.len() < BTRFS_HEADER_SIZE {
            return Err(invalid_fs_data("btrfs node too short for header"));
        }
        Ok(BtrfsHeader {
            _bytenr: u64::from_le_bytes(
                data[0x30..0x38]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            ),
            nritems: u32::from_le_bytes(
                data[0x5D..0x61]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            ),
            level: data[0x61],
        })
    }

    pub(crate) fn parse_leaf_items(data: &[u8], nritems: u32) -> io::Result<Vec<LeafItem>> {
        let mut items = Vec::new();
        for i in 0..nritems {
            let off = BTRFS_HEADER_SIZE + i as usize * LEAF_ITEM_SIZE;
            if off + LEAF_ITEM_SIZE > data.len() {
                break;
            }
            let key = BtrfsKey::parse(&data[off..off + KEY_SIZE])?;
            let data_offset = u32::from_le_bytes(
                data[off + 17..off + 21]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            let data_size = u32::from_le_bytes(
                data[off + 21..off + 25]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            items.push(LeafItem {
                key,
                data_offset,
                data_size,
            });
        }
        Ok(items)
    }

    pub(crate) fn parse_internal_items(data: &[u8], nritems: u32) -> io::Result<Vec<InternalItem>> {
        let mut items = Vec::new();
        for i in 0..nritems {
            let off = BTRFS_HEADER_SIZE + i as usize * INTERNAL_ITEM_SIZE;
            if off + INTERNAL_ITEM_SIZE > data.len() {
                break;
            }
            let key = BtrfsKey::parse(&data[off..off + KEY_SIZE])?;
            let blockptr = u64::from_le_bytes(
                data[off + 17..off + 25]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            items.push(InternalItem { key, blockptr });
        }
        Ok(items)
    }

    pub(crate) fn get_item_data<'a>(node_data: &'a [u8], item: &LeafItem) -> &'a [u8] {
        let start = item.data_offset as usize;
        let end = (start + item.data_size as usize).min(node_data.len());
        &node_data[start..end]
    }

    pub(crate) fn find_items_by_object_and_type(
        items: &[LeafItem],
        objectid: u64,
        key_type: u8,
    ) -> Vec<usize> {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.key.objectid == objectid && item.key.ty == key_type)
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn walk_to_leaf(
        &self,
        root_bytenr: u64,
        search_key: &BtrfsKey,
    ) -> io::Result<(Vec<u8>, Vec<LeafItem>)> {
        let node_data = self.read_logical_block(root_bytenr)?;
        let header = Self::parse_header(&node_data)?;
        if header.level == 0 {
            let items = Self::parse_leaf_items(&node_data, header.nritems)?;
            return Ok((node_data, items));
        }
        let internal = Self::parse_internal_items(&node_data, header.nritems)?;
        let index = internal
            .binary_search_by(|item| item.key.cmp(search_key))
            .unwrap_or_else(|index| index.saturating_sub(1));
        if let Some(item) = internal.get(index.min(internal.len().saturating_sub(1))) {
            self.walk_to_leaf(item.blockptr, search_key)
        } else {
            Err(invalid_fs_data("empty btrfs internal node"))
        }
    }

    pub(crate) fn collect_candidate_leaves(
        &self,
        root_bytenr: u64,
        lower_bound: &BtrfsKey,
        upper_bound: &BtrfsKey,
    ) -> io::Result<Vec<(Vec<u8>, Vec<LeafItem>)>> {
        let mut leaves = Vec::new();
        self.collect_candidate_leaves_from_node(
            root_bytenr,
            lower_bound,
            upper_bound,
            &mut leaves,
        )?;
        leaves.sort_by(|(_, left), (_, right)| {
            left.first()
                .map(|item| &item.key)
                .cmp(&right.first().map(|item| &item.key))
        });
        Ok(leaves)
    }

    fn collect_candidate_leaves_from_node(
        &self,
        node_bytenr: u64,
        lower_bound: &BtrfsKey,
        upper_bound: &BtrfsKey,
        leaves: &mut Vec<(Vec<u8>, Vec<LeafItem>)>,
    ) -> io::Result<()> {
        let node_data = self.read_logical_block(node_bytenr)?;
        let header = Self::parse_header(&node_data)?;
        if header.level == 0 {
            let items = Self::parse_leaf_items(&node_data, header.nritems)?;
            if items.iter().any(|item| {
                item.key.objectid == lower_bound.objectid
                    && item.key.ty == lower_bound.ty
                    && item.key.offset <= upper_bound.offset
            }) {
                leaves.push((node_data, items));
            }
            return Ok(());
        }
        let internal = Self::parse_internal_items(&node_data, header.nritems)?;
        if internal.is_empty() {
            return Ok(());
        }
        for (index, item) in internal.iter().enumerate() {
            if internal
                .get(index + 1)
                .is_some_and(|next| next.key <= *lower_bound)
            {
                continue;
            }
            if item.key > *upper_bound {
                break;
            }
            self.collect_candidate_leaves_from_node(
                item.blockptr,
                lower_bound,
                upper_bound,
                leaves,
            )?;
        }
        Ok(())
    }
}
