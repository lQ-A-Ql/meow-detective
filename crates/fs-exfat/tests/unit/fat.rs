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
fn walk_unexpected_free_cluster_errors() {
    let fat = |cluster: u32| -> io::Result<FatEntry> {
        match cluster {
            2 => Ok(FatEntry::Cluster(3)),
            3 => Ok(FatEntry::Free),
            _ => Ok(FatEntry::Free),
        }
    };

    let result = walk_cluster_chain(2, fat);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("unexpected free cluster"));
}

#[test]
fn walk_chain_limit_errors() {
    let fat = |cluster: u32| -> io::Result<FatEntry> { Ok(FatEntry::Cluster(cluster + 1)) };

    let result = walk_cluster_chain_with_limit(2, Some(2), fat);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("declared cluster count"));
}

#[test]
fn fat_reader_entry_offset() {
    let reader = FatReader::new(24 * 512, 512);
    assert_eq!(reader.entry_offset(0), 24 * 512);
    assert_eq!(reader.entry_offset(2), 24 * 512 + 8);
    assert_eq!(reader.entry_offset(100), 24 * 512 + 400);
}
