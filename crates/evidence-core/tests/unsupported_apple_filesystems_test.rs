use evidence_core::volume::gpt::{
    classify_partition_type, format_guid, partition_type_name, GptPartitionType,
};
use evidence_core::volume::mbr::{classify_mbr_partition_type, MbrPartitionStatus};

#[test]
fn apple_mbr_partition_types_remain_unsupported_metadata() {
    let ufs = classify_mbr_partition_type(0xA8);
    assert_eq!(ufs.name, "Apple UFS");
    assert_eq!(ufs.status, MbrPartitionStatus::Unsupported);

    let hfs = classify_mbr_partition_type(0xAF);
    assert_eq!(hfs.name, "Apple HFS/HFS+");
    assert_eq!(hfs.status, MbrPartitionStatus::Unsupported);
}

#[test]
fn apple_gpt_partition_guids_are_classified_as_metadata() {
    let hfs_guid = [
        0x00, 0x53, 0x46, 0x48, 0x00, 0x00, 0xAA, 0x11, 0xAA, 0x11, 0x00, 0x30, 0x65, 0x43, 0xEC,
        0xAC,
    ];
    let apfs_guid = [
        0xEF, 0x57, 0x34, 0x7C, 0x00, 0x00, 0xAA, 0x11, 0xAA, 0x11, 0x00, 0x30, 0x65, 0x43, 0xEC,
        0xAC,
    ];

    let hfs_type = classify_partition_type(&hfs_guid);
    assert_eq!(hfs_type, GptPartitionType::AppleHfs);
    assert_eq!(partition_type_name(hfs_type), "Apple HFS/HFS+");
    assert_eq!(
        format_guid(&hfs_guid),
        "48465300-0000-11AA-AA11-00306543ECAC"
    );

    let apfs_type = classify_partition_type(&apfs_guid);
    assert_eq!(apfs_type, GptPartitionType::AppleApfs);
    assert_eq!(partition_type_name(apfs_type), "Apple APFS");
    assert_eq!(
        format_guid(&apfs_guid),
        "7C3457EF-0000-11AA-AA11-00306543ECAC"
    );
}
