use crate::reader::btrfs_error;
use crate::BtrfsReader;
use btrfs_driver::inode::Inode;
use btrfs_driver::subvol::EMPTY_SUBVOL_DIR_OBJECTID;
use evidence_core::filesystem::{
    child_nodes_with_parent_path, fs_node, is_special_directory_name, path_components,
    path_is_directory, root_node, FileSystemReader, FsNode, FsTimestamp,
};
use std::io::{self, Read};

impl FileSystemReader for BtrfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let normalized = normalize_path(path);
        let target = self
            .filesystem
            .resolve_path(&normalized)
            .map_err(btrfs_error)?;
        if !target.inode.is_dir() {
            return Err(path_is_directory(path));
        }

        let mut nodes = Vec::new();
        let tree = target.fs(&self.filesystem);
        for entry in tree.read_dir(target.inode.ino).map_err(btrfs_error)? {
            let name = String::from_utf8_lossy(&entry.name).into_owned();
            if is_special_directory_name(&name) {
                continue;
            }
            let child_path = join_path(&normalized, &name);
            let inode = if entry.is_inode() {
                tree.read_inode(entry.ino).map_err(btrfs_error)?
            } else {
                self.filesystem
                    .resolve_path(&child_path)
                    .map_err(btrfs_error)?
                    .inode
            };
            let is_directory = inode.is_dir() || inode.ino == EMPTY_SUBVOL_DIR_OBJECTID;
            nodes.push(node_from_inode(name, &inode, is_directory));
        }
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let data = self
            .filesystem
            .read_path(&normalize_path(path))
            .map_err(btrfs_error)?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let normalized = normalize_path(path);
        let target = self
            .filesystem
            .resolve_path(&normalized)
            .map_err(btrfs_error)?;
        if target.inode.is_dir() {
            return Err(path_is_directory(path));
        }
        let mut output = vec![0u8; length];
        let read = target
            .fs(&self.filesystem)
            .read_at(target.inode.ino, offset, &mut output)
            .map_err(btrfs_error)?;
        output.truncate(read);
        Ok(output)
    }

    fn data_source_name(&self) -> &str {
        "btrfs"
    }
}

fn node_from_inode(name: String, inode: &Inode, is_directory: bool) -> FsNode {
    let mut node = fs_node(
        name,
        is_directory,
        if is_directory { 0 } else { inode.size },
        timestamp(inode.otime),
        timestamp(inode.mtime),
        timestamp(inode.atime),
    );
    node.read_only = inode.mode & 0o222 == 0;
    node.unix_mode = Some(inode.mode);
    node.changed_at = timestamp(inode.ctime);
    node
}

fn timestamp(value: btrfs_driver::inode::Timestamp) -> Option<FsTimestamp> {
    FsTimestamp::from_timestamp(value.sec, value.nsec)
}

fn normalize_path(path: &str) -> String {
    path_components(path).join("/")
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
#[path = "../tests/unit/filesystem.rs"]
mod tests;
