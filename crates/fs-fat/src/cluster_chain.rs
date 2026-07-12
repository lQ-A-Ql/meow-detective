use crate::types::{FatReader, FatType};
use evidence_core::filesystem::{fs_out_of_memory, invalid_fs_data};
use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom};

impl FatReader {
    fn fat_offset(&self) -> u64 {
        self.volume_offset + self.reserved_sectors as u64 * self.bytes_per_sector as u64
    }

    fn cluster_to_offset(&self, cluster: u32) -> u64 {
        self.volume_offset
            + (self.first_data_sector as u64
                + (cluster as u64 - 2) * self.sectors_per_cluster as u64)
                * self.bytes_per_sector as u64
    }

    fn max_cluster(&self) -> u32 {
        self.cluster_count.saturating_add(1)
    }

    fn validate_data_cluster(&self, cluster: u32) -> io::Result<()> {
        if cluster < 2 || cluster > self.max_cluster() {
            return Err(invalid_fs_data(format!(
                "cluster {} out of range 2..={}",
                cluster,
                self.max_cluster()
            )));
        }
        Ok(())
    }

    fn read_fat_entry(&self, cluster: u32) -> io::Result<u32> {
        let (entry_offset, entry_size) = match self.fat_type {
            FatType::Fat12 => (self.fat_offset() + cluster as u64 * 3 / 2, 2),
            FatType::Fat16 => (self.fat_offset() + cluster as u64 * 2, 2),
            FatType::Fat32 => (self.fat_offset() + cluster as u64 * 4, 4),
        };

        let mut buf = vec![0u8; entry_size];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(entry_offset))?;
        reader.read_exact(&mut buf)?;

        let raw = if entry_size == 2 {
            u16::from_le_bytes([buf[0], buf[1]]) as u32
        } else {
            u32::from_le_bytes(buf[0..4].try_into().unwrap_or([0; 4]))
        };
        if self.fat_type == FatType::Fat12 {
            Ok(if cluster & 1 != 0 {
                raw >> 4
            } else {
                raw & 0x0FFF
            })
        } else {
            Ok(raw & 0x0FFF_FFFF)
        }
    }

    fn is_eoc(&self, cluster: u32) -> bool {
        match self.fat_type {
            FatType::Fat12 => cluster >= 0x0FF8,
            FatType::Fat16 => cluster >= 0xFFF8,
            FatType::Fat32 => cluster >= 0x0FFF_FFF8,
        }
    }

    fn is_bad_cluster(&self, cluster: u32) -> bool {
        match self.fat_type {
            FatType::Fat12 => cluster == 0x0FF7,
            FatType::Fat16 => cluster == 0xFFF7,
            FatType::Fat32 => cluster == 0x0FFF_FFF7,
        }
    }

    fn next_cluster(
        &self,
        current: u32,
        start: u32,
        visited: &HashSet<u32>,
    ) -> io::Result<Option<u32>> {
        let next = self.read_fat_entry(current)?;
        if next == 0 {
            return Err(invalid_fs_data(format!(
                "unexpected free cluster {} in chain starting at {}",
                current, start
            )));
        }
        if self.is_bad_cluster(next) {
            return Err(invalid_fs_data(format!(
                "bad cluster marker in chain starting at {} after cluster {}",
                start, current
            )));
        }
        if self.is_eoc(next) {
            return Ok(None);
        }
        self.validate_data_cluster(next)?;
        if visited.contains(&next) {
            return Err(invalid_fs_data(format!(
                "cycle detected in cluster chain: cluster {} points to already-visited cluster {}",
                current, next
            )));
        }
        Ok(Some(next))
    }

    pub(crate) fn walk_cluster_chain(&self, start_cluster: u32) -> io::Result<Vec<u8>> {
        self.validate_data_cluster(start_cluster)?;
        let mut data = Vec::new();
        let mut cluster = start_cluster;
        let mut visited = HashSet::new();

        loop {
            self.validate_data_cluster(cluster)?;
            if !visited.insert(cluster) {
                return Err(invalid_fs_data(format!(
                    "cycle detected in cluster chain at cluster {}",
                    cluster
                )));
            }
            let start = data.len();
            data.resize(start + self.cluster_size as usize, 0);
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(self.cluster_to_offset(cluster)))?;
            reader.read_exact(&mut data[start..])?;
            drop(reader);

            let Some(next) = self.next_cluster(cluster, start_cluster, &visited)? else {
                break;
            };
            if visited.len() > self.cluster_count as usize {
                return Err(invalid_fs_data(format!(
                    "cluster chain exceeds declared cluster count ({})",
                    self.cluster_count
                )));
            }
            cluster = next;
        }
        Ok(data)
    }

    pub(crate) fn read_cluster_chain_range(
        &self,
        start_cluster: u32,
        file_size: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        let bounded_len = bounded_range_len(file_size, offset, length)?;
        if bounded_len == 0 {
            return Ok(Vec::new());
        }

        self.validate_data_cluster(start_cluster)?;
        let mut cluster = start_cluster;
        let mut visited = HashSet::new();
        for _ in 0..offset / self.cluster_size {
            self.validate_data_cluster(cluster)?;
            if !visited.insert(cluster) {
                return Err(invalid_fs_data(format!(
                    "cycle detected in cluster chain at cluster {}",
                    cluster
                )));
            }
            cluster = self
                .next_cluster(cluster, start_cluster, &visited)?
                .ok_or_else(|| premature_chain_end(start_cluster, offset))?;
        }

        self.read_range_from_clusters(
            cluster,
            start_cluster,
            offset % self.cluster_size,
            bounded_len,
            visited,
        )
    }

    fn read_range_from_clusters(
        &self,
        mut cluster: u32,
        start_cluster: u32,
        mut cluster_offset: u64,
        bounded_len: usize,
        mut visited: HashSet<u32>,
    ) -> io::Result<Vec<u8>> {
        let mut data = Vec::with_capacity(bounded_len);
        while data.len() < bounded_len {
            self.validate_data_cluster(cluster)?;
            if !visited.insert(cluster) {
                return Err(invalid_fs_data(format!(
                    "cycle detected in cluster chain at cluster {}",
                    cluster
                )));
            }
            let remaining = bounded_len - data.len();
            let to_read =
                (self.cluster_size.saturating_sub(cluster_offset) as usize).min(remaining);
            let read_offset = self
                .cluster_to_offset(cluster)
                .checked_add(cluster_offset)
                .ok_or_else(|| invalid_fs_data("cluster range offset overflow"))?;
            let start = data.len();
            data.resize(start + to_read, 0);
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(read_offset))?;
            reader.read_exact(&mut data[start..])?;
            drop(reader);

            cluster_offset = 0;
            if data.len() < bounded_len {
                cluster = self
                    .next_cluster(cluster, start_cluster, &visited)?
                    .ok_or_else(|| premature_range_end(start_cluster))?;
                if visited.len() > self.cluster_count as usize {
                    return Err(invalid_fs_data(format!(
                        "cluster chain exceeds declared cluster count ({})",
                        self.cluster_count
                    )));
                }
            }
        }
        Ok(data)
    }
}

fn bounded_range_len(file_size: u64, offset: u64, length: usize) -> io::Result<usize> {
    if offset >= file_size || length == 0 {
        return Ok(0);
    }
    let requested = u64::try_from(length)
        .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
    usize::try_from(requested.min(file_size.saturating_sub(offset)))
        .map_err(|_| fs_out_of_memory("requested range length is too large"))
}

fn premature_chain_end(start_cluster: u32, offset: u64) -> io::Error {
    invalid_fs_data(format!(
        "cluster chain ended before range offset {} in file starting at {}",
        offset, start_cluster
    ))
}

fn premature_range_end(start_cluster: u32) -> io::Error {
    invalid_fs_data(format!(
        "cluster chain ended before reading requested range for file starting at {}",
        start_cluster
    ))
}
