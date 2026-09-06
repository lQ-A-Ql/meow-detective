//! exFAT File Allocation Table (FAT) operations.
//!
//! The FAT is a singly-linked list of cluster indices that describes
//! cluster chains in the Cluster Heap.

use crate::types::*;
use evidence_core::filesystem::invalid_fs_data;
use std::io;

/// Result of reading a FAT entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatEntry {
    /// Free cluster (can be allocated)
    Free,
    /// Next cluster in the chain
    Cluster(u32),
    /// End of chain marker
    EndOfChain,
    /// Bad cluster (media error)
    BadCluster,
    /// Reserved or otherwise invalid FAT marker.
    Reserved(u32),
}

impl FatEntry {
    /// Parse a raw FAT entry value.
    pub fn from_raw(value: u32) -> Self {
        match value {
            FREE_CLUSTER => FatEntry::Free,
            BAD_CLUSTER => FatEntry::BadCluster,
            e if e >= 0xFFFF_FFF8 => FatEntry::EndOfChain,
            e if e == 1 || (0xFFFF_FFF0..=0xFFFF_FFF6).contains(&e) => FatEntry::Reserved(e),
            e if e >= MIN_CLUSTER => FatEntry::Cluster(e),
            _ => FatEntry::Free, // Treat invalid low values as free
        }
    }

    /// Check if this entry represents an end-of-chain marker.
    pub fn is_end_of_chain(&self) -> bool {
        matches!(self, FatEntry::EndOfChain)
    }

    /// Check if this entry is a bad cluster marker.
    pub fn is_bad_cluster(&self) -> bool {
        matches!(self, FatEntry::BadCluster)
    }

    /// Get the next cluster index, if any.
    pub fn next_cluster(&self) -> Option<u32> {
        match self {
            FatEntry::Cluster(c) => Some(*c),
            _ => None,
        }
    }
}

/// Helper to read FAT entries from a volume.
pub struct FatReader {
    fat_offset: u64,
}

impl FatReader {
    /// Create a new FAT reader.
    ///
    /// `fat_offset` is the byte offset of the FAT from the start of the volume.
    pub fn new(fat_offset: u64, _bytes_per_sector: u32) -> Self {
        Self { fat_offset }
    }

    /// Get the byte offset of a FAT entry for a given cluster.
    pub fn entry_offset(&self, cluster: u32) -> u64 {
        self.fat_offset + (cluster as u64 * 4)
    }

    /// Parse a FAT entry from raw bytes.
    pub fn parse_entry(data: &[u8; 4]) -> FatEntry {
        let value = u32::from_le_bytes(*data);
        FatEntry::from_raw(value)
    }
}

/// Walk a cluster chain starting from `start_cluster`.
///
/// Returns a vector of all cluster indices in the chain (including the start cluster).
/// Stops at end-of-chain, bad cluster, free cluster, or cycle detection.
pub fn walk_cluster_chain<F>(start_cluster: u32, read_fat_entry: F) -> io::Result<Vec<u32>>
where
    F: Fn(u32) -> io::Result<FatEntry>,
{
    walk_cluster_chain_with_limit(start_cluster, None, read_fat_entry)
}

/// Walk a cluster chain with an optional maximum chain length.
pub fn walk_cluster_chain_with_limit<F>(
    start_cluster: u32,
    max_clusters: Option<usize>,
    read_fat_entry: F,
) -> io::Result<Vec<u32>>
where
    F: Fn(u32) -> io::Result<FatEntry>,
{
    if start_cluster < MIN_CLUSTER {
        return Ok(Vec::new());
    }

    let mut clusters = Vec::new();
    let mut current = start_cluster;
    let mut visited = std::collections::HashSet::new();

    loop {
        if let Some(max) = max_clusters {
            if clusters.len() >= max {
                return Err(invalid_fs_data(format!(
                    "cluster chain exceeds declared cluster count ({})",
                    max
                )));
            }
        }

        // Cycle detection
        if !visited.insert(current) {
            return Err(invalid_fs_data(format!(
                "cycle detected in cluster chain at cluster {}",
                current
            )));
        }

        clusters.push(current);

        let entry = read_fat_entry(current)?;

        match entry {
            FatEntry::EndOfChain => break,
            FatEntry::Free => {
                return Err(invalid_fs_data(format!(
                    "unexpected free cluster {} in chain starting at {}",
                    current, start_cluster
                )));
            }
            FatEntry::Cluster(next) => {
                // Cycle detection: if next is already visited, it's a cycle
                if visited.contains(&next) {
                    return Err(invalid_fs_data(format!(
                        "cycle detected in cluster chain: cluster {} points to already-visited cluster {}",
                        current, next
                    )));
                }
                current = next;
            }
            FatEntry::BadCluster => {
                return Err(invalid_fs_data(format!(
                    "bad cluster marker in chain starting at {} after cluster {}",
                    start_cluster, current
                )));
            }
            FatEntry::Reserved(value) => {
                return Err(invalid_fs_data(format!(
                    "reserved FAT marker {value:#010x} in chain starting at {}",
                    start_cluster
                )));
            }
        }

        // Sanity check: limit chain length to prevent infinite loops
        if clusters.len() > 100_000_000 {
            return Err(invalid_fs_data("cluster chain too long (>100M clusters)"));
        }
    }

    Ok(clusters)
}

#[cfg(test)]
#[path = "../tests/unit/fat.rs"]
mod tests;
