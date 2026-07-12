//! exFAT filesystem reader.
//!
//! Implements the `FileSystemReader` trait for exFAT formatted volumes.
//! Based on the Microsoft exFAT specification.

pub mod boot;
pub mod dir;
pub mod fat;
pub mod types;

use boot::ExfatBootSector;
use dir::FileEntrySet;
use evidence_core::filesystem::{
    child_nodes_with_parent_path_with_separator, file_not_found, fs_node_with_attributes,
    fs_out_of_memory, is_special_directory_name, path_components, path_is_directory,
    path_is_not_directory, path_not_found, root_node, truncate_data_to_declared_size,
    unsupported_fs, FileSystemReader, FsNode,
};
use evidence_core::EvidenceReader;
use fat::{FatEntry, FatReader};
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};
use types::{ATTR_HIDDEN, ATTR_SYSTEM, MIN_CLUSTER};

/// exFAT filesystem reader.
pub struct ExfatReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    boot: ExfatBootSector,
    /// Offset of the exFAT volume within the evidence (e.g., partition offset).
    volume_offset: u64,
}

impl ExfatReader {
    /// Open an exFAT volume at the given offset.
    ///
    /// Reads and validates the boot sector, then prepares for filesystem operations.
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut boot_buf = [0u8; 512];
        reader.read_exact(&mut boot_buf)?;

        let boot = ExfatBootSector::parse(&boot_buf)?;

        // Validate revision (must be 1.xx)
        if boot.revision_major() != 1 {
            return Err(unsupported_fs(format!(
                "unsupported exFAT revision {}.{}",
                boot.revision_major(),
                boot.revision_minor()
            )));
        }

        Ok(Self {
            reader: RefCell::new(reader),
            boot,
            volume_offset: offset,
        })
    }

    /// Validate that a cluster index is addressable in this volume's cluster heap.
    fn validate_cluster(&self, cluster: u32) -> io::Result<()> {
        let max_cluster = self.boot.cluster_count.saturating_add(1);
        if cluster < MIN_CLUSTER || cluster > max_cluster {
            return Err(evidence_core::filesystem::invalid_fs_data(format!(
                "cluster {} out of range 2..={}",
                cluster, max_cluster
            )));
        }
        Ok(())
    }

    /// Read a FAT entry for a given cluster.
    fn read_fat_entry(&self, cluster: u32) -> io::Result<FatEntry> {
        self.validate_cluster(cluster)?;

        let fat_reader = fat::FatReader::new(
            self.volume_offset + self.boot.fat_byte_offset(),
            self.boot.bytes_per_sector(),
        );

        let offset = fat_reader.entry_offset(cluster);
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;

        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;

        Ok(FatReader::parse_entry(&buf))
    }

    /// Walk a cluster chain and return all cluster indices.
    fn walk_cluster_chain(&self, start_cluster: u32) -> io::Result<Vec<u32>> {
        self.validate_cluster(start_cluster)?;
        fat::walk_cluster_chain_with_limit(
            start_cluster,
            Some(self.boot.cluster_count as usize),
            |cluster| self.read_fat_entry(cluster),
        )
    }

    /// Convert a cluster index to an absolute byte offset in the evidence.
    fn cluster_to_abs_offset(&self, cluster: u32) -> u64 {
        self.volume_offset + self.boot.cluster_to_offset(cluster)
    }

    /// Read data from a contiguous cluster run that does not use the FAT chain.
    fn read_no_fat_chain_data(&self, start_cluster: u32, data_length: u64) -> io::Result<Vec<u8>> {
        self.validate_cluster(start_cluster)?;
        let cluster_size = self.boot.cluster_size();
        let clusters_to_read = data_length.div_ceil(cluster_size).max(1);
        let max_cluster = self.boot.cluster_count.saturating_add(1) as u64;
        let end_cluster = start_cluster as u64 + clusters_to_read - 1;
        if end_cluster > max_cluster {
            return Err(evidence_core::filesystem::invalid_fs_data(format!(
                "NoFatChain run starting at cluster {} exceeds declared cluster count ({})",
                start_cluster, self.boot.cluster_count
            )));
        }

        let mut data = Vec::with_capacity((clusters_to_read * cluster_size) as usize);
        for cluster in start_cluster..=end_cluster as u32 {
            let offset = self.cluster_to_abs_offset(cluster);
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(offset))?;

            let mut buf = vec![0u8; cluster_size as usize];
            reader.read_exact(&mut buf)?;
            data.extend_from_slice(&buf);
        }

        Ok(data)
    }

    /// Read data from a cluster chain.
    fn read_cluster_chain_data(&self, start_cluster: u32) -> io::Result<Vec<u8>> {
        let clusters = self.walk_cluster_chain(start_cluster)?;
        let cluster_size = self.boot.cluster_size() as usize;
        let mut data = Vec::with_capacity(clusters.len() * cluster_size);

        for &cluster in &clusters {
            let offset = self.cluster_to_abs_offset(cluster);
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(offset))?;

            let mut buf = vec![0u8; cluster_size];
            reader.read_exact(&mut buf)?;
            data.extend_from_slice(&buf);
        }

        Ok(data)
    }

    /// Read directory entries from a directory's cluster chain.
    fn read_directory_entries(&self, cluster: u32) -> io::Result<Vec<FileEntrySet>> {
        let data = self.read_cluster_chain_data(cluster)?;
        dir::parse_directory_entries(&data)
    }

    fn read_entry_data(
        &self,
        cluster: u32,
        data_length: u64,
        no_fat_chain: bool,
    ) -> io::Result<Vec<u8>> {
        if data_length == 0 {
            return Ok(Vec::new());
        }
        if no_fat_chain {
            self.read_no_fat_chain_data(cluster, data_length)
        } else {
            self.read_cluster_chain_data(cluster)
        }
    }

    fn bounded_range_len(data_length: u64, offset: u64, length: usize) -> io::Result<usize> {
        if offset >= data_length || length == 0 {
            return Ok(0);
        }

        let requested = u64::try_from(length)
            .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
        let bounded = requested.min(data_length.saturating_sub(offset));
        usize::try_from(bounded)
            .map_err(|_| fs_out_of_memory("requested range length is too large"))
    }

    fn read_no_fat_chain_range(
        &self,
        start_cluster: u32,
        data_length: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        let bounded_len = Self::bounded_range_len(data_length, offset, length)?;
        if bounded_len == 0 {
            return Ok(Vec::new());
        }

        self.validate_cluster(start_cluster)?;
        let cluster_size = self.boot.cluster_size();
        let clusters_to_read = data_length.div_ceil(cluster_size).max(1);
        let max_cluster = self.boot.cluster_count.saturating_add(1) as u64;
        let end_cluster = start_cluster as u64 + clusters_to_read - 1;
        if end_cluster > max_cluster {
            return Err(evidence_core::filesystem::invalid_fs_data(format!(
                "NoFatChain run starting at cluster {} exceeds declared cluster count ({})",
                start_cluster, self.boot.cluster_count
            )));
        }

        let read_offset = self
            .cluster_to_abs_offset(start_cluster)
            .checked_add(offset)
            .ok_or_else(|| evidence_core::filesystem::invalid_fs_data("range offset overflow"))?;
        let mut data = vec![0u8; bounded_len];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(read_offset))?;
        reader.read_exact(&mut data)?;
        Ok(data)
    }

    fn next_cluster_in_chain(
        &self,
        current: u32,
        start_cluster: u32,
        visited: &std::collections::HashSet<u32>,
    ) -> io::Result<Option<u32>> {
        match self.read_fat_entry(current)? {
            FatEntry::EndOfChain => Ok(None),
            FatEntry::BadCluster => Err(evidence_core::filesystem::invalid_fs_data(format!(
                "bad cluster marker in chain starting at {} after cluster {}",
                start_cluster, current
            ))),
            FatEntry::Free => Err(evidence_core::filesystem::invalid_fs_data(format!(
                "unexpected free cluster {} in chain starting at {}",
                current, start_cluster
            ))),
            FatEntry::Cluster(next) => {
                self.validate_cluster(next)?;
                if visited.contains(&next) {
                    return Err(evidence_core::filesystem::invalid_fs_data(format!(
                        "cycle detected in cluster chain: cluster {} points to already-visited cluster {}",
                        current, next
                    )));
                }
                Ok(Some(next))
            }
        }
    }

    fn read_cluster_chain_range(
        &self,
        start_cluster: u32,
        data_length: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        let bounded_len = Self::bounded_range_len(data_length, offset, length)?;
        if bounded_len == 0 {
            return Ok(Vec::new());
        }

        self.validate_cluster(start_cluster)?;
        let cluster_size = self.boot.cluster_size();
        let first_cluster_index = offset / cluster_size;
        let mut cluster_offset = offset % cluster_size;
        let mut cluster = start_cluster;
        let mut visited = std::collections::HashSet::new();

        for _ in 0..first_cluster_index {
            self.validate_cluster(cluster)?;
            if !visited.insert(cluster) {
                return Err(evidence_core::filesystem::invalid_fs_data(format!(
                    "cycle detected in cluster chain at cluster {}",
                    cluster
                )));
            }
            cluster = self
                .next_cluster_in_chain(cluster, start_cluster, &visited)?
                .ok_or_else(|| {
                    evidence_core::filesystem::invalid_fs_data(format!(
                        "cluster chain ended before range offset {} in file starting at {}",
                        offset, start_cluster
                    ))
                })?;
        }

        let mut data = Vec::with_capacity(bounded_len);
        let mut remaining = bounded_len;
        while remaining > 0 {
            self.validate_cluster(cluster)?;
            if !visited.insert(cluster) {
                return Err(evidence_core::filesystem::invalid_fs_data(format!(
                    "cycle detected in cluster chain at cluster {}",
                    cluster
                )));
            }

            let available_in_cluster = cluster_size.saturating_sub(cluster_offset);
            let to_read = (available_in_cluster as usize).min(remaining);
            let read_offset = self
                .cluster_to_abs_offset(cluster)
                .checked_add(cluster_offset)
                .ok_or_else(|| {
                    evidence_core::filesystem::invalid_fs_data("range offset overflow")
                })?;
            let start = data.len();
            data.resize(start + to_read, 0);
            {
                let mut reader = self.reader.borrow_mut();
                reader.seek(SeekFrom::Start(read_offset))?;
                reader.read_exact(&mut data[start..])?;
            }

            remaining -= to_read;
            cluster_offset = 0;
            if remaining == 0 {
                break;
            }

            cluster = self
                .next_cluster_in_chain(cluster, start_cluster, &visited)?
                .ok_or_else(|| {
                    evidence_core::filesystem::invalid_fs_data(format!(
                        "cluster chain ended before reading requested range for file starting at {}",
                        start_cluster
                    ))
                })?;

            if visited.len() > self.boot.cluster_count as usize {
                return Err(evidence_core::filesystem::invalid_fs_data(format!(
                    "cluster chain exceeds declared cluster count ({})",
                    self.boot.cluster_count
                )));
            }
        }

        Ok(data)
    }

    fn read_entry_range(
        &self,
        cluster: u32,
        data_length: u64,
        no_fat_chain: bool,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        if no_fat_chain {
            self.read_no_fat_chain_range(cluster, data_length, offset, length)
        } else {
            self.read_cluster_chain_range(cluster, data_length, offset, length)
        }
    }

    /// Resolve a path to a (cluster, is_dir, size, no_fat_chain) tuple.
    ///
    /// Returns None if the path doesn't exist.
    fn resolve_path(&self, path: &str) -> io::Result<Option<(u32, bool, u64, bool)>> {
        let components = path_components(path);

        if components.is_empty() {
            // Root directory
            return Ok(Some((self.boot.first_cluster_of_root, true, 0, false)));
        }

        let mut current_cluster = self.boot.first_cluster_of_root;
        let mut is_dir = true;
        let mut no_fat_chain = false;

        for (i, component) in components.iter().enumerate() {
            if !is_dir {
                return Ok(None); // Can't traverse into a file
            }

            let entries = self.read_directory_entries(current_cluster)?;
            let lower_component = component.to_lowercase();

            let found = entries
                .iter()
                .find(|e| e.name.to_lowercase() == lower_component);

            match found {
                Some(entry) => {
                    let is_last = i == components.len() - 1;
                    current_cluster = entry.first_cluster;
                    is_dir = entry.is_directory();
                    let size = if is_dir { 0 } else { entry.valid_data_length };
                    no_fat_chain = entry.no_fat_chain;

                    if is_last {
                        return Ok(Some((current_cluster, is_dir, size, no_fat_chain)));
                    }
                }
                None => return Ok(None),
            }
        }

        Ok(Some((current_cluster, is_dir, 0, no_fat_chain)))
    }

    /// Read a file range by path without materializing the whole file.
    pub fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let (cluster, is_dir, size, no_fat_chain) = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;

        if is_dir {
            return Err(path_is_directory(path));
        }

        self.read_entry_range(cluster, size, no_fat_chain, offset, length)
    }

    /// Open a file and return a seekable in-memory cursor.
    fn open_file_cursor(&self, path: &str) -> io::Result<io::Cursor<Vec<u8>>> {
        let (cluster, is_dir, size, no_fat_chain) = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;

        if is_dir {
            return Err(path_is_directory(path));
        }

        let data = truncate_data_to_declared_size(
            self.read_entry_data(cluster, size, no_fat_chain)?,
            size,
        );
        Ok(io::Cursor::new(data))
    }
}

impl FileSystemReader for ExfatReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let (cluster, is_dir, _, _) = self
            .resolve_path(path)?
            .ok_or_else(|| path_not_found(path))?;

        if !is_dir {
            return Err(path_is_not_directory(path));
        }

        let entries = self.read_directory_entries(cluster)?;
        let mut nodes = Vec::new();

        for entry in entries {
            // Skip special entries
            if is_special_directory_name(&entry.name) {
                continue;
            }

            let is_dir = entry.is_directory();

            nodes.push(fs_node_with_attributes(
                entry.name,
                is_dir,
                entry.valid_data_length,
                entry.attributes & ATTR_HIDDEN != 0,
                entry.attributes & ATTR_SYSTEM != 0,
                false,
                entry.created_at,
                entry.modified_at,
                entry.accessed_at,
            ));
        }

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

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
