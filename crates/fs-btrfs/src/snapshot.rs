//! Btrfs snapshot enumeration, reading, and diff.
//!
//! Btrfs snapshots are subvolumes that share data with their parent via
//! copy-on-write.  Each snapshot is represented as a ROOT_ITEM / ROOT_BACKREF
//! pair in the root tree, with its own per-subvolume file B-tree.
//!
//! This module provides:
//! - Listing available snapshots
//! - Reading the file tree of a single snapshot
//! - Computing a file-level diff between two snapshots

use crate::{BtrfsReader, BtrfsSubvol, FS_TREE_OBJECTID, FT_DIR, ROOT_BACKREF_KEY, ROOT_ITEM_KEY};
use evidence_core::filesystem::{fs_node, invalid_fs_data, FsNode};
use std::collections::HashMap;
use std::io;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Difference between two Btrfs snapshot file trees.
#[derive(Debug, Clone)]
pub struct SnapshotDiff {
    /// Files present in `snap2` but absent in `snap1`.
    pub added: Vec<FsNode>,
    /// Files present in `snap1` but absent in `snap2`.
    pub removed: Vec<FsNode>,
    /// Files present in both but with different size.
    pub changed: Vec<SnapshotFileChange>,
}

/// A single changed file between two snapshots.
#[derive(Debug, Clone)]
pub struct SnapshotFileChange {
    pub path: String,
    pub old_size: u64,
    pub new_size: u64,
}

// ---------------------------------------------------------------------------
// BtrfsReader snapshot methods
// ---------------------------------------------------------------------------

impl BtrfsReader {
    /// List all available snapshots by scanning the root tree for ROOT_BACKREF
    /// items with non-empty names, excluding the default FS_TREE subvolume.
    pub fn list_snapshots(&self) -> io::Result<Vec<BtrfsSubvol>> {
        let root_data = self.read_logical_block(self.root_tree_logical)?;
        let header = Self::parse_header(&root_data)?;

        let mut root_items: HashMap<u64, (u64, u64)> = HashMap::new();
        let mut root_names: HashMap<u64, String> = HashMap::new();

        if header.level == 0 {
            Self::scan_snapshot_leaf(&root_data, header.nritems, &mut root_items, &mut root_names)?;
        } else {
            let internal = Self::parse_internal_items(&root_data, header.nritems)?;
            for ii in &internal {
                let child = self.read_logical_block(ii.blockptr)?;
                let ch = Self::parse_header(&child)?;
                if ch.level == 0 {
                    Self::scan_snapshot_leaf(&child, ch.nritems, &mut root_items, &mut root_names)?;
                } else {
                    let si = Self::parse_internal_items(&child, ch.nritems)?;
                    for s in &si {
                        let leaf = self.read_logical_block(s.blockptr)?;
                        let lh = Self::parse_header(&leaf)?;
                        if lh.level == 0 {
                            Self::scan_snapshot_leaf(
                                &leaf,
                                lh.nritems,
                                &mut root_items,
                                &mut root_names,
                            )?;
                        }
                    }
                }
            }
        }

        let mut snapshots = Vec::new();
        for (id, (bytenr, root_dirid)) in &root_items {
            // Exclude the default FS_TREE — it is not a snapshot.
            if *id == FS_TREE_OBJECTID {
                continue;
            }
            if let Some(name) = root_names.get(id) {
                snapshots.push(BtrfsSubvol {
                    id: *id,
                    name: name.clone(),
                    root_dirid: *root_dirid,
                    tree_root_bytenr: *bytenr,
                });
            }
        }
        Ok(snapshots)
    }

    /// Read the full file tree of a snapshot identified by its root object id.
    ///
    /// Returns a flat list of `FsNode` values with paths relative to the
    /// snapshot root.
    pub fn read_snapshot(&self, snap_id: u64) -> io::Result<Vec<FsNode>> {
        // Find the snapshot in the subvolume list (including snapshots).
        let root_data = self.read_logical_block(self.root_tree_logical)?;
        let header = Self::parse_header(&root_data)?;
        let (bytenr, root_dirid) = self.find_root_item_by_id(&root_data, header, snap_id)?;

        self.list_all_files_in_tree(bytenr, root_dirid, "")
    }

    /// Compute a file-level diff between two snapshots identified by their
    /// root object ids.
    ///
    /// Returns a `SnapshotDiff` with three categories: `added`, `removed`,
    /// and `changed`.
    pub fn diff_snapshots(&self, snap1_id: u64, snap2_id: u64) -> io::Result<SnapshotDiff> {
        let files1 = self.read_snapshot(snap1_id)?;
        let files2 = self.read_snapshot(snap2_id)?;
        Ok(diff_file_trees(&files1, &files2))
    }

    // -- internal helpers ----------------------------------------------------

    /// Scan a root-tree leaf node for snapshot-relevant ROOT_ITEM and
    /// ROOT_BACKREF entries.
    fn scan_snapshot_leaf(
        data: &[u8],
        nritems: u32,
        root_items: &mut HashMap<u64, (u64, u64)>,
        root_names: &mut HashMap<u64, String>,
    ) -> io::Result<()> {
        let items = Self::parse_leaf_items(data, nritems)?;
        for item in &items {
            if item.key.ty == ROOT_ITEM_KEY {
                let rd = Self::get_item_data(data, item);
                if rd.len() >= 184 {
                    let bytenr = u64::from_le_bytes(
                        rd[176..184]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    );
                    let root_dirid = u64::from_le_bytes(
                        rd[168..176]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    );
                    root_items.insert(item.key.objectid, (bytenr, root_dirid));
                }
            } else if item.key.ty == ROOT_BACKREF_KEY {
                let rb = Self::get_item_data(data, item);
                if rb.len() >= 18 {
                    let name_len = u16::from_le_bytes(
                        rb[16..18]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    ) as usize;
                    if rb.len() >= 18 + name_len {
                        let name = String::from_utf8_lossy(&rb[18..18 + name_len]).to_string();
                        root_names.insert(item.key.objectid, name);
                    }
                }
            }
        }
        Ok(())
    }

    /// Walk the root tree to find the ROOT_ITEM for a specific root id.
    fn find_root_item_by_id(
        &self,
        root_data: &[u8],
        header: crate::BtrfsHeader,
        root_id: u64,
    ) -> io::Result<(u64, u64)> {
        if header.level == 0 {
            return Self::find_in_leaf(root_data, header.nritems, root_id);
        }
        let internal = Self::parse_internal_items(root_data, header.nritems)?;
        for ii in &internal {
            let child = self.read_logical_block(ii.blockptr)?;
            let ch = Self::parse_header(&child)?;
            if ch.level == 0 {
                if let Ok(result) = Self::find_in_leaf(&child, ch.nritems, root_id) {
                    return Ok(result);
                }
            } else {
                let si = Self::parse_internal_items(&child, ch.nritems)?;
                for s in &si {
                    let leaf = self.read_logical_block(s.blockptr)?;
                    let lh = Self::parse_header(&leaf)?;
                    if lh.level == 0 {
                        if let Ok(result) = Self::find_in_leaf(&leaf, lh.nritems, root_id) {
                            return Ok(result);
                        }
                    }
                }
            }
        }
        Err(evidence_core::filesystem::invalid_fs_data(format!(
            "snapshot root id {} not found in root tree",
            root_id
        )))
    }

    fn find_in_leaf(data: &[u8], nritems: u32, root_id: u64) -> io::Result<(u64, u64)> {
        let items = Self::parse_leaf_items(data, nritems)?;
        for item in &items {
            if item.key.objectid == root_id && item.key.ty == ROOT_ITEM_KEY {
                let rd = Self::get_item_data(data, item);
                if rd.len() >= 184 {
                    let bytenr = u64::from_le_bytes(
                        rd[176..184]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    );
                    let root_dirid = u64::from_le_bytes(
                        rd[168..176]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    );
                    return Ok((bytenr, root_dirid));
                }
            }
        }
        Err(evidence_core::filesystem::invalid_fs_data(format!(
            "root item for id {} not found in leaf",
            root_id
        )))
    }

    /// Recursively list all files and directories under a given tree root
    /// directory, producing a flat list of `FsNode` values.
    fn list_all_files_in_tree(
        &self,
        tree_root_bytenr: u64,
        dir_objectid: u64,
        parent_path: &str,
    ) -> io::Result<Vec<FsNode>> {
        let entries = self.list_dir_entries(tree_root_bytenr, dir_objectid)?;
        let mut nodes = Vec::new();

        for (name, inode_obj, file_type) in entries {
            if evidence_core::filesystem::is_special_directory_name(&name) {
                continue;
            }
            let child_path = if parent_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", parent_path, name)
            };

            let is_dir = file_type == FT_DIR;
            let metadata = self
                .get_inode_metadata(tree_root_bytenr, inode_obj)
                .ok()
                .flatten();
            let size = if !is_dir {
                metadata.map(|value| value.size).unwrap_or_default()
            } else {
                0
            };

            let mut node = fs_node(
                name.clone(),
                is_dir,
                size,
                metadata.and_then(|value| value.created_at),
                metadata.and_then(|value| value.modified_at),
                metadata.and_then(|value| value.accessed_at),
            );
            if let Some(metadata) = metadata {
                node.read_only = metadata.read_only;
                node.changed_at = metadata.changed_at;
            }
            node.path = child_path.clone();
            nodes.push(node);

            if is_dir {
                let children =
                    self.list_all_files_in_tree(tree_root_bytenr, inode_obj, &child_path)?;
                nodes.extend(children);
            }
        }

        Ok(nodes)
    }
}

// ---------------------------------------------------------------------------
// Diff helper
// ---------------------------------------------------------------------------

/// Compute a diff between two flat file trees keyed by path.
fn diff_file_trees(files1: &[FsNode], files2: &[FsNode]) -> SnapshotDiff {
    let map1: HashMap<&str, &FsNode> = files1.iter().map(|n| (n.path.as_str(), n)).collect();
    let map2: HashMap<&str, &FsNode> = files2.iter().map(|n| (n.path.as_str(), n)).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (&path, &node) in &map2 {
        if !map1.contains_key(path) {
            added.push(node.clone());
        }
    }

    for (&path, &node) in &map1 {
        if !map2.contains_key(path) {
            removed.push(node.clone());
        }
    }

    for (&path, &node1) in &map1 {
        if let Some(&node2) = map2.get(path) {
            if node1.size != node2.size {
                changed.push(SnapshotFileChange {
                    path: path.to_string(),
                    old_size: node1.size,
                    new_size: node2.size,
                });
            }
        }
    }

    added.sort_by(|a, b| a.path.cmp(&b.path));
    removed.sort_by(|a, b| a.path.cmp(&b.path));
    changed.sort_by(|a, b| a.path.cmp(&b.path));

    SnapshotDiff {
        added,
        removed,
        changed,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "../tests/unit/snapshot.rs"]
mod tests;
