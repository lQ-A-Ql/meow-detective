use crate::XfsReader;
use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node_without_timestamps,
    is_special_directory_name, join_child_path, path_is_directory, path_is_not_directory,
    path_not_found, root_node, FileSystemReader, FsNode,
};
use std::io::{self, Read};

impl FileSystemReader for XfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let (ino, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| path_not_found(path))?;
        if !is_dir {
            return Err(path_is_not_directory(path));
        }

        let entries = self.read_directory_entries(ino)?;
        for entry in &entries {
            if entry.is_dir && !is_special_directory_name(&entry.name) {
                self.cache_directory_path(join_child_path(path, &entry.name), entry.inode);
            }
        }
        let nodes: Vec<FsNode> = entries
            .into_iter()
            .filter(|entry| !is_special_directory_name(&entry.name))
            .map(|entry| fs_node_without_timestamps(entry.name, entry.is_dir, entry.size))
            .collect();
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let (ino, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        if is_dir {
            return Err(path_is_directory(path));
        }
        Ok(Box::new(io::Cursor::new(self.read_file_content(ino)?)))
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let (ino, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        if is_dir {
            return Err(path_is_directory(path));
        }
        self.read_file_content_range(ino, offset, length)
    }

    fn data_source_name(&self) -> &str {
        "xfs"
    }
}
