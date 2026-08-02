use super::is_bitlocker_partition;
use persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord;

fn partition(
    kind_label: &str,
    filesystem: Option<&str>,
    status: &str,
) -> DataSourcePartitionRecord {
    DataSourcePartitionRecord {
        id: "partition-1".to_string(),
        data_source_id: "source-1".to_string(),
        partition_index: 1,
        name: "partition".to_string(),
        type_guid: None,
        offset: 0,
        length: 1024,
        kind_label: kind_label.to_string(),
        filesystem: filesystem.map(str::to_string),
        status: status.to_string(),
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
fn recognizes_all_persisted_bitlocker_markers() {
    assert!(is_bitlocker_partition(&partition(
        "BitLocker",
        None,
        "supported"
    )));
    assert!(is_bitlocker_partition(&partition(
        "Basic data",
        Some("BITLOCKER"),
        "supported"
    )));
    assert!(is_bitlocker_partition(&partition(
        "Basic data",
        None,
        "locked"
    )));
    assert!(is_bitlocker_partition(&partition(
        "Basic data",
        None,
        "encrypted_bitlocker"
    )));
    assert!(!is_bitlocker_partition(&partition(
        "Basic data",
        Some("NTFS"),
        "supported"
    )));
}
