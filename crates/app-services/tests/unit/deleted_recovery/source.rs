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

fn bitlocker_partition() -> DataSourcePartitionRecord {
    let mut value = partition("NTFS");
    value.kind_label = "BitLocker".to_string();
    value
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

#[test]
fn ready_bitlocker_partition_routes_as_ntfs_recovery() {
    let partition = bitlocker_partition();
    assert!(crate::partition_capabilities::is_bitlocker_partition(
        &partition
    ));
    assert_eq!(
        recovery_filesystem(&partition, DataSourcePlatform::Windows),
        Some(("ntfs", ImageFilesystemKind::Ntfs)),
    );
}

#[test]
fn locked_bitlocker_runtime_maps_to_an_actionable_typed_error() {
    let error =
        map_bitlocker_runtime_error(crate::bitlocker_runtime::BitLockerRuntimeError::Locked);
    assert!(matches!(error, DeletedRecoveryError::BitLockerLocked));
    assert_eq!(
        transport::ServiceErrorCategory::code(&error),
        Some("RECOVERY_BITLOCKER_LOCKED")
    );
}
