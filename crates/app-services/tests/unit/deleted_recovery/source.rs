use super::*;

fn partition(filesystem: &str) -> DataSourcePartitionRecord {
    DataSourcePartitionRecord {
        id: format!("partition-{filesystem}"),
        data_source_id: "source-1".to_string(),
        partition_index: 1,
        name: filesystem.to_string(),
        kind_label: filesystem.to_string(),
        status: "ready".to_string(),
        type_guid: None,
        offset: 0,
        length: 4096,
        filesystem: Some(filesystem.to_string()),
        unlock_hint: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        lvm_pv_offsets_json: None,
        lvm_pv_sources_json: None,
    }
}

#[test]
fn windows_recovery_accepts_only_ntfs() {
    assert_eq!(
        recovery_filesystem(&partition("NTFS"), DataSourcePlatform::Windows),
        Some(("ntfs", ImageFilesystemKind::Ntfs)),
    );
    assert_eq!(
        recovery_filesystem(&partition("XFS"), DataSourcePlatform::Windows),
        None,
    );
}

#[test]
fn linux_recovery_accepts_only_ext4_and_xfs() {
    assert_eq!(
        recovery_filesystem(&partition("Ext4"), DataSourcePlatform::Linux),
        Some(("ext4", ImageFilesystemKind::Ext4)),
    );
    assert_eq!(
        recovery_filesystem(&partition("XFS"), DataSourcePlatform::Linux),
        Some(("xfs", ImageFilesystemKind::Xfs)),
    );
    assert_eq!(
        recovery_filesystem(&partition("NTFS"), DataSourcePlatform::Linux),
        None,
    );
}
