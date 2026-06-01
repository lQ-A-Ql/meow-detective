//! exFAT File Allocation Table (FAT) operations.
//!
//! The FAT is a singly-linked list of cluster indices that describes
//! cluster chains in the Cluster Heap.

use crate::types::*;
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
}

impl FatEntry {
    /// Parse a raw FAT entry value.
    pub fn from_raw(value: u32) -> Self {
        match value {
            FREE_CLUSTER => FatEntry::Free,
            BAD_CLUSTER => FatEntry::BadCluster,
            e if e >= 0xFFFF_FFF8 => FatEntry::EndOfChain,
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
    if start_cluster < MIN_CLUSTER {
        return Ok(Vec::new());
    }

    let mut clusters = Vec::new();
    let mut current = start_cluster;
    let mut visited = std::collections::HashSet::new();

    loop {
        // Cycle detection
        if !visited.insert(current) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("cycle detected in cluster chain at cluster {}", current),
            ));
        }

        clusters.push(current);

        let entry = read_fat_entry(current)?;

        match entry {
            FatEntry::EndOfChain | FatEntry::BadCluster => break,
            FatEntry::Free => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "unexpected free cluster {} in chain starting at {}",
                        current, start_cluster
                    ),
                ));
            }
            FatEntry::Cluster(next) => {
                // Cycle detection: if next is already visited, it's a cycle
                if visited.contains(&next) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "cycle detected in cluster chain: cluster {} points to already-visited cluster {}",
                            current, next
                        ),
                    ));
                }
                current = next;
            }
        }

        // Sanity check: limit chain length to prevent infinite loops
        if clusters.len() > 100_000_000 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cluster chain too long (>100M clusters)",
            ));
        }
    }

    Ok(clusters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fat_entry_from_raw() {
        assert_eq!(FatEntry::from_raw(0), FatEntry::Free);
        assert_eq!(FatEntry::from_raw(0xFFFF_FFFF), FatEntry::EndOfChain);
        assert_eq!(FatEntry::from_raw(0xFFFF_FFF8), FatEntry::EndOfChain);
        assert_eq!(FatEntry::from_raw(0xFFFF_FFF7), FatEntry::BadCluster);
        assert_eq!(FatEntry::from_raw(2), FatEntry::Cluster(2));
        assert_eq!(FatEntry::from_raw(100), FatEntry::Cluster(100));
    }

    #[test]
    fn fat_entry_methods() {
        let eoc = FatEntry::EndOfChain;
        assert!(eoc.is_end_of_chain());
        assert!(!eoc.is_bad_cluster());
        assert_eq!(eoc.next_cluster(), None);

        let cluster = FatEntry::Cluster(5);
        assert!(!cluster.is_end_of_chain());
        assert_eq!(cluster.next_cluster(), Some(5));
    }

    #[test]
    fn walk_simple_chain() {
        // Simulate FAT: 2 -> 3 -> 4 -> EOC
        let fat = |cluster: u32| -> io::Result<FatEntry> {
            match cluster {
                2 => Ok(FatEntry::Cluster(3)),
                3 => Ok(FatEntry::Cluster(4)),
                4 => Ok(FatEntry::EndOfChain),
                _ => Ok(FatEntry::Free),
            }
        };

        let chain = walk_cluster_chain(2, fat).unwrap();
        assert_eq!(chain, vec![2, 3, 4]);
    }

    #[test]
    fn walk_single_cluster_chain() {
        // Single cluster: 5 -> EOC
        let fat = |cluster: u32| -> io::Result<FatEntry> {
            match cluster {
                5 => Ok(FatEntry::EndOfChain),
                _ => Ok(FatEntry::Free),
            }
        };

        let chain = walk_cluster_chain(5, fat).unwrap();
        assert_eq!(chain, vec![5]);
    }

    #[test]
    fn walk_empty_chain() {
        let fat = |_cluster: u32| -> io::Result<FatEntry> { Ok(FatEntry::Free) };

        let chain = walk_cluster_chain(0, fat).unwrap();
        assert!(chain.is_empty());
    }

    #[test]
    fn walk_cycle_detected() {
        // Cycle: 2 -> 3 -> 2
        let fat = |cluster: u32| -> io::Result<FatEntry> {
            match cluster {
                2 => Ok(FatEntry::Cluster(3)),
                3 => Ok(FatEntry::Cluster(2)),
                _ => Ok(FatEntry::Free),
            }
        };

        let result = walk_cluster_chain(2, fat);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn fat_reader_entry_offset() {
        let reader = FatReader::new(24 * 512, 512);
        assert_eq!(reader.entry_offset(0), 24 * 512);
        assert_eq!(reader.entry_offset(2), 24 * 512 + 8);
        assert_eq!(reader.entry_offset(100), 24 * 512 + 400);
    }
}
