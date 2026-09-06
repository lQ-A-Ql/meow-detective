use crate::fat::{self, FatEntry};
use crate::reader::ExfatReader;
use evidence_core::filesystem::{fs_out_of_memory, invalid_fs_data};
use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom};

const MAX_DIRECTORY_DATA_BYTES: u64 = 64 * 1024 * 1024;

impl ExfatReader {
    pub(crate) fn read_entry_data(
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
            let length = usize::try_from(data_length)
                .map_err(|_| fs_out_of_memory("file data length exceeds addressable memory"))?;
            self.validate_chain_matches_length(cluster, data_length)?;
            self.read_cluster_chain_range(cluster, data_length, 0, length)
        }
    }

    fn validate_chain_matches_length(
        &self,
        start_cluster: u32,
        data_length: u64,
    ) -> io::Result<()> {
        self.validate_cluster(start_cluster)?;
        let cluster_size = self.boot.cluster_size();
        let required_clusters = data_length.div_ceil(cluster_size).max(1);
        let mut current = start_cluster;
        let mut visited = HashSet::new();
        for index in 0..required_clusters {
            if !visited.insert(current) {
                return Err(invalid_fs_data(format!(
                    "cycle detected in cluster chain at cluster {}",
                    current
                )));
            }
            let next = self.read_fat_entry(current)?;
            if index + 1 < required_clusters {
                current = match next {
                    FatEntry::Cluster(next) => {
                        self.validate_cluster(next)?;
                        next
                    }
                    FatEntry::EndOfChain => {
                        return Err(invalid_fs_data(
                            "cluster chain ended before the declared file data length",
                        ));
                    }
                    FatEntry::BadCluster => {
                        return Err(invalid_fs_data("bad cluster marker in file chain"));
                    }
                    FatEntry::Free => {
                        return Err(invalid_fs_data("free cluster marker in file chain"));
                    }
                    FatEntry::Reserved(value) => {
                        return Err(invalid_fs_data(format!(
                            "reserved FAT marker {value:#010x} in file chain"
                        )));
                    }
                };
            } else if !matches!(next, FatEntry::EndOfChain) {
                return Err(invalid_fs_data(format!(
                    "cluster chain extends beyond the declared cluster count/data length ({})",
                    self.boot.cluster_count
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn read_cluster_chain_data(&self, start_cluster: u32) -> io::Result<Vec<u8>> {
        let cluster_size = usize::try_from(self.boot.cluster_size())
            .map_err(|_| fs_out_of_memory("exFAT cluster size exceeds addressable memory"))?;
        let max_clusters =
            usize::try_from(MAX_DIRECTORY_DATA_BYTES / self.boot.cluster_size().max(1))
                .unwrap_or(0);
        if max_clusters == 0 {
            return Err(fs_out_of_memory(
                "exFAT cluster size exceeds directory read limit",
            ));
        }
        let clusters =
            fat::walk_cluster_chain_with_limit(start_cluster, Some(max_clusters), |cluster| {
                self.read_fat_entry(cluster)
            })?;
        let capacity = clusters
            .len()
            .checked_mul(cluster_size)
            .ok_or_else(|| fs_out_of_memory("directory data size overflows memory capacity"))?;
        if u64::try_from(capacity).unwrap_or(u64::MAX) > MAX_DIRECTORY_DATA_BYTES {
            return Err(fs_out_of_memory(format!(
                "directory data exceeds {} MiB",
                MAX_DIRECTORY_DATA_BYTES / (1024 * 1024)
            )));
        }
        let mut data = Vec::with_capacity(capacity);

        for cluster in clusters {
            let offset = self.cluster_to_abs_offset(cluster);
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(offset))?;

            let mut buf = vec![0u8; cluster_size];
            reader.read_exact(&mut buf)?;
            data.extend_from_slice(&buf);
        }

        Ok(data)
    }

    pub(crate) fn read_no_fat_chain_data(
        &self,
        start_cluster: u32,
        data_length: u64,
    ) -> io::Result<Vec<u8>> {
        let (end_cluster, cluster_size) =
            self.validate_contiguous_run(start_cluster, data_length)?;
        let capacity = data_length.div_ceil(cluster_size).max(1) * cluster_size;
        let capacity = usize::try_from(capacity)
            .map_err(|_| fs_out_of_memory("contiguous cluster run is too large"))?;
        let mut data = Vec::with_capacity(capacity);

        for cluster in start_cluster..=end_cluster {
            let offset = self.cluster_to_abs_offset(cluster);
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(offset))?;

            let mut buf = vec![0u8; cluster_size as usize];
            reader.read_exact(&mut buf)?;
            data.extend_from_slice(&buf);
        }

        Ok(data)
    }

    fn validate_contiguous_run(
        &self,
        start_cluster: u32,
        data_length: u64,
    ) -> io::Result<(u32, u64)> {
        self.validate_cluster(start_cluster)?;
        let cluster_size = self.boot.cluster_size();
        let clusters_to_read = data_length.div_ceil(cluster_size).max(1);
        let max_cluster = self.boot.cluster_count.saturating_add(1) as u64;
        let end_cluster = start_cluster as u64 + clusters_to_read - 1;
        if end_cluster > max_cluster {
            return Err(invalid_fs_data(format!(
                "NoFatChain run starting at cluster {} exceeds declared cluster count ({})",
                start_cluster, self.boot.cluster_count
            )));
        }
        Ok((end_cluster as u32, cluster_size))
    }

    pub(crate) fn read_entry_range(
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

        self.validate_contiguous_run(start_cluster, data_length)?;
        let read_offset = self
            .cluster_to_abs_offset(start_cluster)
            .checked_add(offset)
            .ok_or_else(|| invalid_fs_data("range offset overflow"))?;
        let mut data = vec![0u8; bounded_len];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(read_offset))?;
        reader.read_exact(&mut data)?;
        Ok(data)
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

        let (mut cluster, mut visited) = self.seek_chain_to_offset(start_cluster, offset)?;
        let cluster_size = self.boot.cluster_size();
        let mut cluster_offset = offset % cluster_size;
        let mut data = Vec::with_capacity(bounded_len);

        while data.len() < bounded_len {
            if !visited.insert(cluster) {
                return Err(invalid_fs_data(format!(
                    "cycle detected in cluster chain at cluster {}",
                    cluster
                )));
            }
            self.read_range_from_cluster(
                cluster,
                cluster_offset,
                bounded_len.saturating_sub(data.len()),
                &mut data,
            )?;
            cluster_offset = 0;
            if data.len() == bounded_len {
                break;
            }

            cluster = self
                .next_cluster_in_chain(cluster, start_cluster, &visited)?
                .ok_or_else(|| {
                    invalid_fs_data(format!(
                        "cluster chain ended before reading requested range for file starting at {}",
                        start_cluster
                    ))
                })?;
            if visited.len() > self.boot.cluster_count as usize {
                return Err(invalid_fs_data(format!(
                    "cluster chain exceeds declared cluster count ({})",
                    self.boot.cluster_count
                )));
            }
        }

        Ok(data)
    }

    fn seek_chain_to_offset(
        &self,
        start_cluster: u32,
        offset: u64,
    ) -> io::Result<(u32, HashSet<u32>)> {
        self.validate_cluster(start_cluster)?;
        let first_cluster_index = offset / self.boot.cluster_size();
        let mut cluster = start_cluster;
        let mut visited = HashSet::new();

        for _ in 0..first_cluster_index {
            self.validate_cluster(cluster)?;
            if !visited.insert(cluster) {
                return Err(invalid_fs_data(format!(
                    "cycle detected in cluster chain at cluster {}",
                    cluster
                )));
            }
            cluster = self
                .next_cluster_in_chain(cluster, start_cluster, &visited)?
                .ok_or_else(|| {
                    invalid_fs_data(format!(
                        "cluster chain ended before range offset {} in file starting at {}",
                        offset, start_cluster
                    ))
                })?;
        }

        Ok((cluster, visited))
    }

    fn read_range_from_cluster(
        &self,
        cluster: u32,
        cluster_offset: u64,
        remaining: usize,
        data: &mut Vec<u8>,
    ) -> io::Result<()> {
        self.validate_cluster(cluster)?;
        let available = self.boot.cluster_size().saturating_sub(cluster_offset);
        let to_read = (available as usize).min(remaining);
        let read_offset = self
            .cluster_to_abs_offset(cluster)
            .checked_add(cluster_offset)
            .ok_or_else(|| invalid_fs_data("range offset overflow"))?;
        let start = data.len();
        data.resize(start + to_read, 0);

        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(read_offset))?;
        reader.read_exact(&mut data[start..])
    }
}
