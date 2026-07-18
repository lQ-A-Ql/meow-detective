use crate::XfsReader;
use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, invalid_fs_data, is_special_directory_name,
    join_child_path, path_is_directory, path_is_not_directory, path_not_found, root_node,
    FileSystemReader, FsNode,
};
use std::io::{self, Read};

impl FileSystemReader for XfsReader {
    fn root(&self) -> io::Result<FsNode> {
        let metadata = self.inode_metadata(self.root_ino)?;
        if !metadata.is_dir {
            return Err(invalid_fs_data(format!(
                "XFS root inode {} is not a directory",
                self.root_ino
            )));
        }
        let mut root = root_node();
        root.created_at = metadata.created_at;
        root.modified_at = metadata.modified_at;
        root.accessed_at = metadata.accessed_at;
        root.changed_at = metadata.changed_at;
        Ok(root)
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
            .map(|entry| {
                let metadata = entry.metadata.as_ref();
                FsNode {
                    name: entry.name,
                    path: String::new(),
                    is_dir: entry.is_dir,
                    size: if entry.is_dir {
                        0
                    } else {
                        metadata.map(|value| value.size).unwrap_or(0)
                    },
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: metadata.and_then(|value| value.created_at),
                    modified_at: metadata.and_then(|value| value.modified_at),
                    accessed_at: metadata.and_then(|value| value.accessed_at),
                    changed_at: metadata.and_then(|value| value.changed_at),
                }
            })
            .collect();
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn take_diagnostics(&self) -> Vec<evidence_core::filesystem::FileSystemDiagnostic> {
        std::mem::take(&mut *self.diagnostics.borrow_mut())
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
