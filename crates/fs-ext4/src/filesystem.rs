use crate::format::{S_IFDIR, S_IFLNK};
use crate::Ext4Reader;
use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node_without_timestamps,
    is_special_directory_name, path_is_directory, path_not_found, root_node, FileSystemReader,
    FsNode,
};
use std::io::{self, Read};

impl FileSystemReader for Ext4Reader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let (inode_number, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| path_not_found(path))?;
        if !is_dir {
            return Err(evidence_core::filesystem::path_is_not_directory(path));
        }
        let mut nodes = Vec::new();
        for (name, child_inode, file_type) in self.read_directory_entries(inode_number)? {
            if is_special_directory_name(&name) {
                continue;
            }
            let fallback_is_dir = file_type == 2;
            let node = self
                .read_inode(child_inode)
                .and_then(|inode| {
                    let is_dir = Self::inode_mode(&inode) & 0xF000 == S_IFDIR;
                    let size = if is_dir { 0 } else { Self::inode_size(&inode)? };
                    let mut node = fs_node_without_timestamps(name.clone(), is_dir, size);
                    let mode = Self::inode_mode(&inode);
                    node.read_only = mode & 0o222 == 0;
                    node.created_at = Self::inode_created_at(&inode);
                    node.modified_at = Self::inode_modified_at(&inode);
                    node.accessed_at = Self::inode_accessed_at(&inode);
                    node.changed_at = Self::inode_changed_at(&inode);
                    Ok(node)
                })
                .unwrap_or_else(|_| fs_node_without_timestamps(name, fallback_is_dir, 0));
            nodes.push(node);
        }
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let inode = self.resolve_file_inode(path)?;
        if Self::inode_mode(&inode) & 0xF000 == S_IFLNK {
            return Ok(Box::new(io::Cursor::new(
                self.read_symlink_target(&inode)?.into_bytes(),
            )));
        }
        let data = self.read_extent_data(Self::inode_i_block(&inode), Self::inode_size(&inode)?)?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let inode = self.resolve_file_inode(path)?;
        if Self::inode_mode(&inode) & 0xF000 == S_IFLNK {
            let target = self.read_symlink_target(&inode)?.into_bytes();
            let start = usize::try_from(offset)
                .ok()
                .map(|start| start.min(target.len()))
                .unwrap_or(target.len());
            let end = start.saturating_add(length).min(target.len());
            return Ok(target[start..end].to_vec());
        }
        self.read_extent_data_range(
            Self::inode_i_block(&inode),
            Self::inode_size(&inode)?,
            offset,
            length,
        )
    }

    fn data_source_name(&self) -> &str {
        "ext4"
    }
}

impl Ext4Reader {
    fn resolve_file_inode(&self, path: &str) -> io::Result<Vec<u8>> {
        let (inode_number, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        if is_dir {
            return Err(path_is_directory(path));
        }
        self.read_inode(inode_number)
    }
}
