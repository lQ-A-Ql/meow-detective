use crate::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource,
};
use crate::staging::{PartitionEntry, PartitionStatus as StagingPartitionStatus, StagingManifest};

fn candidate(index: usize, kind: ImageFilesystemKind) -> ImageFilesystemCandidate {
    ImageFilesystemCandidate {
        partition_index: Some(index),
        partition_name: Some(format!("Partition {index}")),
        kind,
        offset: index as u64 * 512,
        length: Some(4096),
        source: ImageFilesystemSource::MbrPartition,
        lvm_identity: None,
    }
}

fn manifest_partition(index: usize, fs_kind: &str) -> PartitionEntry {
    PartitionEntry {
        index,
        name: format!("Partition {index}"),
        fs_kind: fs_kind.to_string(),
        staging_db: format!("enum_partition_{index}.db"),
        status: StagingPartitionStatus::Pending,
        file_count: 99,
        dir_count: 88,
        total_size: 77,
        last_path: Some("stale".to_string()),
        completed_at: None,
        error: Some("reader build failed".to_string()),
    }
}

#[test]
fn locked_bitlocker_partition_completes_as_metadata_only() {
    let mut manifest = StagingManifest::create("source", "volume.dd", "Raw");
    manifest.partitions = vec![manifest_partition(1, "BitLocker")];
    let candidates = vec![candidate(1, ImageFilesystemKind::BitLocker)];

    super::mark_locked_partitions_done(&mut manifest, &candidates);

    let partition = &manifest.partitions[0];
    assert_eq!(partition.status, StagingPartitionStatus::Done);
    assert_eq!(partition.file_count, 0);
    assert_eq!(partition.dir_count, 0);
    assert_eq!(partition.total_size, 0);
    assert!(partition.completed_at.is_some());
    assert!(partition.error.is_none());
}

#[test]
fn unlocked_filesystem_partition_remains_pending() {
    let mut manifest = StagingManifest::create("source", "volume.dd", "Raw");
    manifest.partitions = vec![manifest_partition(1, "NTFS")];
    let candidates = vec![candidate(1, ImageFilesystemKind::Ntfs)];

    super::mark_locked_partitions_done(&mut manifest, &candidates);

    assert_eq!(
        manifest.partitions[0].status,
        StagingPartitionStatus::Pending
    );
    assert_eq!(
        manifest.partitions[0].error.as_deref(),
        Some("reader build failed")
    );
}
