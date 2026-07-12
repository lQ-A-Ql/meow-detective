use crate::datasource_service::{self, ImageFilesystemKind, PartitionStatus};

pub fn format_partition_root_name(
    candidate: &datasource_service::ImageFilesystemCandidate,
) -> String {
    let fs_label = match candidate.kind {
        ImageFilesystemKind::Ntfs => "NTFS",
        ImageFilesystemKind::Fat => "FAT",
        ImageFilesystemKind::BitLocker => "BitLocker",
        ImageFilesystemKind::Ext4 => "Ext4",
        ImageFilesystemKind::Xfs => "XFS",
        ImageFilesystemKind::Btrfs => "Btrfs",
        ImageFilesystemKind::LvmPool => "LVM",
    };
    match candidate.partition_index {
        Some(index) => datasource_service::partition_display_name(
            index,
            fs_label,
            candidate.partition_name.as_deref(),
            None,
        ),
        None => {
            datasource_service::volume_display_name(fs_label, candidate.partition_name.as_deref())
        }
    }
}

pub fn format_partition_record_root_name(
    partition: &datasource_service::PartitionRecord,
) -> String {
    let name = partition.name.trim();
    if name.is_empty()
        || name.eq_ignore_ascii_case("unknown")
        || matches!(name, "/" | "\\" | "." | "..")
    {
        return datasource_service::partition_display_name(
            partition.index,
            &partition.kind_label,
            None,
            None,
        );
    }
    name.to_string()
}

pub fn format_partition_progress_detail(
    completed_partitions: u32,
    total_partitions: u32,
    partition_progress: u32,
    current_partition: &str,
    detail: &str,
) -> String {
    format!(
        "[partition-progress] {}|{}|{}|{}|{}",
        completed_partitions,
        total_partitions.max(1),
        partition_progress.min(100),
        current_partition,
        detail
    )
}

pub(crate) fn partition_status_label(status: PartitionStatus) -> &'static str {
    match status {
        PartitionStatus::Supported => "queued",
        PartitionStatus::Expanded => "redirected",
        PartitionStatus::EncryptedBitLocker => "locked",
        PartitionStatus::Unsupported => "unsupported",
    }
}
