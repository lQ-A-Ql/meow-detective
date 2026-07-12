use crate::reader::ExfatReader;
use evidence_core::filesystem::{fs_out_of_memory, invalid_fs_data};
use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom};

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
            self.read_cluster_chain_data(cluster)
        }
    }

    pub(crate) fn read_cluster_chain_data(&self, start_cluster: u32) -> io::Result<Vec<u8>> {
        let clusters = self.walk_cluster_chain(start_cluster)?;
        let cluster_size = self.boot.cluster_size() as usize;
        let mut data = Vec::with_capacity(clusters.len() * cluster_size);

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

    fn read_no_fat_chain_data(&self, start_cluster: u32, data_length: u64) -> io::Result<Vec<u8>> {
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
