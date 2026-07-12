use crate::reader::ExfatReader;
use crate::types::{ATTR_HIDDEN, ATTR_SYSTEM};
use evidence_core::filesystem::{
    child_nodes_with_parent_path_with_separator, fs_node_with_attributes,
    is_special_directory_name, path_is_not_directory, path_not_found, root_node, FileSystemReader,
    FsNode,
};
use std::io::{self, Read};

impl FileSystemReader for ExfatReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let entry = self
            .resolve_path(path)?
            .ok_or_else(|| path_not_found(path))?;
        if !entry.is_dir {
            return Err(path_is_not_directory(path));
        }

        let nodes: Vec<FsNode> = self
            .read_directory_entries(entry.cluster)?
            .into_iter()
            .filter(|entry| !is_special_directory_name(&entry.name))
            .map(|entry| {
                let is_dir = entry.is_directory();
                fs_node_with_attributes(
                    entry.name,
                    is_dir,
                    entry.valid_data_length,
                    entry.attributes & ATTR_HIDDEN != 0,
                    entry.attributes & ATTR_SYSTEM != 0,
                    false,
                    entry.created_at,
                    entry.modified_at,
                    entry.accessed_at,
                )
            })
            .collect();

        Ok(child_nodes_with_parent_path_with_separator(
            nodes, path, '\\',
        ))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(self.open_file_cursor(path)?))
    }

    fn open_file_seekable(&self, path: &str) -> io::Result<Box<dyn evidence_core::ReadSeek>> {
        Ok(Box::new(self.open_file_cursor(path)?))
    }

    fn data_source_name(&self) -> &str {
        "exFAT"
    }
}
