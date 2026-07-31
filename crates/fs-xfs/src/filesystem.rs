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
        root.read_only = metadata.read_only;
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
            if is_special_directory_name(&entry.name) {
                continue;
            }
            let child_path = join_child_path(path, &entry.name);
            if entry.is_dir {
                self.cache_directory_path(child_path, entry.inode);
            } else {
                self.cache_file_path(child_path, entry.inode);
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
                    read_only: metadata.map(|value| value.read_only).unwrap_or(false),
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

    fn directory_locators(&self) -> Vec<evidence_core::FileSystemDirectoryLocator> {
        self.exported_directory_locators()
    }

    fn seed_directory_locators(
        &self,
        locators: &[evidence_core::FileSystemDirectoryLocator],
    ) -> io::Result<()> {
        self.seed_persisted_directory_locators(locators)
    }

    fn file_locators(&self) -> Vec<evidence_core::FileSystemFileLocator> {
        self.exported_file_locators()
    }

    fn seed_file_locators(
        &self,
        locators: &[evidence_core::FileSystemFileLocator],
    ) -> io::Result<()> {
        self.seed_persisted_file_locators(locators)
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let resolved = self
            .resolve_path_with_inode(path)?
            .ok_or_else(|| file_not_found(path))?;
        if resolved.is_dir {
            return Err(path_is_directory(path));
        }
        let content = match resolved.inode {
            Some(inode) => self.read_file_content_from_inode(&inode)?,
            None => self.read_file_content(resolved.inode_number)?,
        };
        Ok(Box::new(io::Cursor::new(content)))
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let resolved = self
            .resolve_path_with_inode(path)?
            .ok_or_else(|| file_not_found(path))?;
        if resolved.is_dir {
            return Err(path_is_directory(path));
        }
        match resolved.inode {
            Some(inode) => self.read_file_content_range_from_inode(&inode, offset, length),
            None => self.read_file_content_range(resolved.inode_number, offset, length),
        }
    }

    fn read_metrics(&self) -> evidence_core::FileSystemReadMetrics {
        *self.read_metrics.borrow()
    }

    fn data_source_name(&self) -> &str {
        "xfs"
    }
}
