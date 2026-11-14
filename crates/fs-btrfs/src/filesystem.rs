use crate::reader::btrfs_error;
use crate::BtrfsReader;
use btrfs_driver::inode::Inode;
use btrfs_driver::subvol::EMPTY_SUBVOL_DIR_OBJECTID;
use evidence_core::filesystem::{
    child_nodes_with_parent_path, fs_node, is_special_directory_name, path_components,
    path_is_directory, root_node, FileSystemReader, FsNode, FsTimestamp,
};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

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

    fn open_file_seekable(
        &self,
        path: &str,
    ) -> io::Result<Box<dyn evidence_core::filesystem::ReadSeek>> {
        let normalized = normalize_path(path);
        let target = self
            .filesystem
            .resolve_path(&normalized)
            .map_err(btrfs_error)?;
        if target.inode.is_dir() {
            return Err(path_is_directory(path));
        }
        Ok(Box::new(BtrfsSeekableFile {
            filesystem: Arc::clone(&self.filesystem),
            path: normalized,
            position: 0,
            size: target.inode.size,
        }))
    }

    fn data_source_name(&self) -> &str {
        "btrfs"
    }
}

struct BtrfsSeekableFile {
    filesystem: Arc<btrfs_driver::Filesystem>,
    path: String,
    position: u64,
    size: u64,
}

impl Read for BtrfsSeekableFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || self.position >= self.size {
            return Ok(0);
        }
        let remaining = usize::try_from(self.size - self.position).unwrap_or(usize::MAX);
        let length = buffer.len().min(remaining);
        let target = self
            .filesystem
            .resolve_path(&self.path)
            .map_err(btrfs_error)?;
        let read = target
            .fs(&self.filesystem)
            .read_at(target.inode.ino, self.position, &mut buffer[..length])
            .map_err(btrfs_error)?;
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for BtrfsSeekableFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.size) + i128::from(delta),
        };
        if next < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before the start of the Btrfs file",
            ));
        }
        self.position = u64::try_from(next).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek beyond the addressable range",
            )
        })?;
        Ok(self.position)
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
