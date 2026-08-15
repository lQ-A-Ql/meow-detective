use std::io::{self, Read, Seek, SeekFrom};

use evidence_core::filesystem::{
    child_nodes_with_parent_path, fs_node_without_timestamps, path_is_directory,
    path_is_not_directory, root_node, FileSystemReader, FsNode,
};

use crate::{F2fsError, F2fsReader};

impl FileSystemReader for F2fsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let inode = self.resolve_path(path).map_err(F2fsError::into_io)?;
        if !inode.is_directory() {
            return Err(path_is_not_directory(path));
        }
        let entries = self.read_directory(&inode).map_err(F2fsError::into_io)?;
        let mut nodes = Vec::with_capacity(entries.len());
        for entry in entries {
            let inode = self.read_inode(entry.inode).map_err(F2fsError::into_io)?;
            let is_directory = inode.is_directory();
            if entry.file_type == 2 && !is_directory {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "directory entry `{}` has inconsistent file type",
                        entry.name
                    ),
                ));
            }
            let mut node = fs_node_without_timestamps(
                entry.name,
                is_directory,
                if is_directory { 0 } else { inode.size },
            );
            node.read_only = inode.mode & 0o222 == 0;
            node.unix_mode = Some(u32::from(inode.mode));
            node.encrypted = inode.is_encrypted();
            nodes.push(node);
        }
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let inode = self.resolve_path(path).map_err(F2fsError::into_io)?;
        if inode.is_directory() {
            return Err(path_is_directory(path));
        }
        Ok(Box::new(
            self.open_file_data(&inode).map_err(F2fsError::into_io)?,
        ))
    }

    fn open_file_seekable(&self, path: &str) -> io::Result<Box<dyn evidence_core::ReadSeek>> {
        let inode = self.resolve_path(path).map_err(F2fsError::into_io)?;
        if inode.is_directory() {
            return Err(path_is_directory(path));
        }
        Ok(Box::new(
            self.open_file_data(&inode).map_err(F2fsError::into_io)?,
        ))
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let inode = self.resolve_path(path).map_err(F2fsError::into_io)?;
        if inode.is_directory() {
            return Err(path_is_directory(path));
        }
        if offset >= inode.size || length == 0 {
            return Ok(Vec::new());
        }
        let mut file = self.open_file_data(&inode).map_err(F2fsError::into_io)?;
        file.seek(SeekFrom::Start(offset))?;
        let bounded = length.min(usize::try_from(inode.size - offset).unwrap_or(usize::MAX));
        let mut bytes = vec![0u8; bounded];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn data_source_name(&self) -> &str {
        "F2FS"
    }
}
