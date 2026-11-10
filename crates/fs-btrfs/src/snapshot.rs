//! Btrfs subvolume and snapshot enumeration.

use crate::reader::btrfs_error;
use crate::{BtrfsReader, BtrfsSubvol};
use btrfs_driver::subvol::Subvolume;
use btrfs_driver::Filesystem;
use evidence_core::filesystem::{fs_node, FsNode};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;

const FIRST_FREE_OBJECTID: u64 = 256;
const MAX_SNAPSHOT_NODES: usize = 1_000_000;
const MAX_SNAPSHOT_DIRECTORY_DEPTH: usize = 256;
const MAX_SNAPSHOT_PATH_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct SnapshotDiff {
    pub added: Vec<FsNode>,
    pub removed: Vec<FsNode>,
    pub changed: Vec<SnapshotFileChange>,
}

#[derive(Debug, Clone)]
pub struct SnapshotFileChange {
    pub path: String,
    pub old_size: u64,
    pub new_size: u64,
}

impl BtrfsReader {
    pub fn list_snapshots(&self) -> io::Result<Vec<BtrfsSubvol>> {
        self.filesystem
            .subvolumes()
            .map_err(btrfs_error)
            .map(|subvolumes| {
                subvolumes
                    .into_iter()
                    .filter(|subvolume| !subvolume.is_default())
                    .map(snapshot_from_subvolume)
                    .collect()
            })
    }

    pub fn read_snapshot(&self, snapshot_id: u64) -> io::Result<Vec<FsNode>> {
        let root = self
            .filesystem
            .open_subvolume(snapshot_id)
            .map_err(btrfs_error)?;
        self.list_all_files_in_tree(Arc::new(root))
    }

    pub fn diff_snapshots(&self, first_id: u64, second_id: u64) -> io::Result<SnapshotDiff> {
        let first = self.read_snapshot(first_id)?;
        let second = self.read_snapshot(second_id)?;
        Ok(diff_file_trees(&first, &second))
    }

    fn list_all_files_in_tree(&self, root: Arc<Filesystem>) -> io::Result<Vec<FsNode>> {
        let mut nodes = Vec::new();
        let mut pending = vec![SnapshotDirectory {
            filesystem: root,
            inode: FIRST_FREE_OBJECTID,
            path: String::new(),
            tree_path: String::new(),
            depth: 0,
        }];

        while let Some(directory) = pending.pop() {
            if directory.depth > MAX_SNAPSHOT_DIRECTORY_DEPTH
                || directory.path.len() > MAX_SNAPSHOT_PATH_BYTES
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Btrfs snapshot directory exceeds safety limits",
                ));
            }
            let entries = directory
                .filesystem
                .read_dir(directory.inode)
                .map_err(btrfs_error)?;
            for entry in entries {
                let name = String::from_utf8_lossy(&entry.name).into_owned();
                if evidence_core::filesystem::is_special_directory_name(&name) {
                    continue;
                }
                let child_path = join_path(&directory.path, &name);
                let child_tree_path = join_path(&directory.tree_path, &name);
                if child_path.len() > MAX_SNAPSHOT_PATH_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Btrfs snapshot path exceeds safety limits",
                    ));
                }
                if nodes.len() >= MAX_SNAPSHOT_NODES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Btrfs snapshot contains too many nodes",
                    ));
                }

                let child = if entry.is_inode() {
                    SnapshotTarget {
                        filesystem: Arc::clone(&directory.filesystem),
                        inode: directory
                            .filesystem
                            .read_inode(entry.ino)
                            .map_err(btrfs_error)?,
                        traversable: true,
                    }
                } else {
                    let target = directory
                        .filesystem
                        .resolve_path(&child_tree_path)
                        .map_err(btrfs_error)?;
                    let traversable = target.tree.is_some();
                    SnapshotTarget {
                        filesystem: target
                            .tree
                            .map(Arc::new)
                            .unwrap_or_else(|| Arc::clone(&directory.filesystem)),
                        inode: target.inode,
                        traversable,
                    }
                };
                let is_dir = child.inode.is_dir();
                let mut node = fs_node(
                    name,
                    is_dir,
                    if is_dir { 0 } else { child.inode.size },
                    timestamp(child.inode.otime),
                    timestamp(child.inode.mtime),
                    timestamp(child.inode.atime),
                );
                node.path = child_path.clone();
                node.read_only = child.inode.mode & 0o222 == 0;
                node.unix_mode = Some(child.inode.mode);
                node.changed_at = timestamp(child.inode.ctime);
                nodes.push(node);
                if is_dir && child.traversable {
                    pending.push(SnapshotDirectory {
                        filesystem: child.filesystem,
                        inode: child.inode.ino,
                        path: child_path,
                        tree_path: if entry.is_inode() {
                            child_tree_path
                        } else {
                            String::new()
                        },
                        depth: directory.depth + 1,
                    });
                }
            }
        }
        Ok(nodes)
    }
}

struct SnapshotDirectory {
    filesystem: Arc<Filesystem>,
    inode: u64,
    path: String,
    tree_path: String,
    depth: usize,
}

struct SnapshotTarget {
    filesystem: Arc<Filesystem>,
    inode: btrfs_driver::inode::Inode,
    traversable: bool,
}

fn snapshot_from_subvolume(subvolume: Subvolume) -> BtrfsSubvol {
    let name = if subvolume.name.is_empty() {
        subvolume.path.rsplit('/').next().unwrap_or("/").to_string()
    } else {
        String::from_utf8_lossy(&subvolume.name).into_owned()
    };
    BtrfsSubvol {
        id: subvolume.id,
        name,
        root_dirid: FIRST_FREE_OBJECTID,
        tree_root_bytenr: subvolume.bytenr,
    }
}

fn timestamp(
    value: btrfs_driver::inode::Timestamp,
) -> Option<evidence_core::filesystem::FsTimestamp> {
    evidence_core::filesystem::FsTimestamp::from_timestamp(value.sec, value.nsec)
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

fn diff_file_trees(first: &[FsNode], second: &[FsNode]) -> SnapshotDiff {
    let first_by_path: HashMap<&str, &FsNode> = first
        .iter()
        .map(|node| (node.path.as_str(), node))
        .collect();
    let second_by_path: HashMap<&str, &FsNode> = second
        .iter()
        .map(|node| (node.path.as_str(), node))
        .collect();
    let mut added = second
        .iter()
        .filter(|node| !first_by_path.contains_key(node.path.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let mut removed = first
        .iter()
        .filter(|node| !second_by_path.contains_key(node.path.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let mut changed = first
        .iter()
        .filter_map(|old| {
            let new = second_by_path.get(old.path.as_str())?;
            (old.size != new.size).then(|| SnapshotFileChange {
                path: old.path.clone(),
                old_size: old.size,
                new_size: new.size,
            })
        })
        .collect::<Vec<_>>();
    added.sort_by(|left, right| left.path.cmp(&right.path));
    removed.sort_by(|left, right| left.path.cmp(&right.path));
    changed.sort_by(|left, right| left.path.cmp(&right.path));
    SnapshotDiff {
        added,
        removed,
        changed,
    }
}

#[cfg(test)]
#[path = "../tests/unit/snapshot.rs"]
mod tests;
