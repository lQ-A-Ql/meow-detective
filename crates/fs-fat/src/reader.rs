use crate::types::{FatReader, FatType};
use evidence_core::filesystem::{
    file_not_found, path_is_directory, root_node, truncate_data_to_declared_size, FileSystemReader,
    FsNode,
};
use std::io::{self, Read};

impl FatReader {
    pub fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        match self.resolve_path_cluster(path)? {
            Some((_, false, 0)) => Ok(Vec::new()),
            Some((cluster, false, size)) => {
                self.read_cluster_chain_range(cluster, size, offset, length)
            }
            Some((_, true, _)) => Err(path_is_directory(path)),
            None => Err(file_not_found(path)),
        }
    }

    fn open_file_cursor(&self, path: &str) -> io::Result<io::Cursor<Vec<u8>>> {
        match self.resolve_path_cluster(path)? {
            Some((_, false, 0)) => Ok(io::Cursor::new(Vec::new())),
            Some((cluster, false, size)) => {
                let data = truncate_data_to_declared_size(self.walk_cluster_chain(cluster)?, size);
                Ok(io::Cursor::new(data))
            }
            Some((_, true, _)) => Err(path_is_directory(path)),
            None => Err(file_not_found(path)),
        }
    }
}

impl FileSystemReader for FatReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        match self.resolve_path_cluster(path)? {
            Some((cluster, true, _)) => {
                let data = if cluster == 0 {
                    self.read_root_data()?
                } else {
                    self.walk_cluster_chain(cluster)?
                };
                Ok(Self::parse_directory_entries(&data, path))
            }
            _ => Ok(Vec::new()),
        }
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(self.open_file_cursor(path)?))
    }

    fn open_file_seekable(&self, path: &str) -> io::Result<Box<dyn evidence_core::ReadSeek>> {
        Ok(Box::new(self.open_file_cursor(path)?))
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        FatReader::read_file_range(self, path, offset, length)
    }

    fn data_source_name(&self) -> &str {
        match self.fat_type {
            FatType::Fat12 => "FAT12",
            FatType::Fat16 => "FAT16",
            FatType::Fat32 => "FAT32",
        }
    }
}
