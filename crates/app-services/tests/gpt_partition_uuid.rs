use app_services::{datasource_service::detect_image_filesystem, file_service};
use domain::DataSourceId;
use persistence_sqlite::repositories::partition_repo::PartitionRepo;
use std::io::Cursor;

fn image(unique: [u8; 16]) -> Vec<u8> {
    let mut bytes = vec![0u8; 8192];
    bytes[510..512].copy_from_slice(&[0x55, 0xaa]);
    bytes[450] = 0xee;
    bytes[454..458].copy_from_slice(&1u32.to_le_bytes());
    bytes[458..462].copy_from_slice(&15u32.to_le_bytes());
    let header = &mut bytes[512..1024];
    header[..8].copy_from_slice(b"EFI PART");
    header[12..16].copy_from_slice(&92u32.to_le_bytes());
    header[56..72].copy_from_slice(&[
        0x78, 0x56, 0x34, 0x12, 0xbc, 0x9a, 0xf0, 0xde, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ]);
    header[72..80].copy_from_slice(&2u64.to_le_bytes());
    header[80..84].copy_from_slice(&1u32.to_le_bytes());
    header[84..88].copy_from_slice(&128u32.to_le_bytes());
    let partition = &mut bytes[1024..1152];
    partition[..16].copy_from_slice(&[
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99,
        0xc7,
    ]);
    partition[16..32].copy_from_slice(&unique);
    partition[32..40].copy_from_slice(&4u64.to_le_bytes());
    partition[40..48].copy_from_slice(&15u64.to_le_bytes());
    bytes[2048 + 1024 + 56..2048 + 1024 + 58].copy_from_slice(&[0x53, 0xef]);
    bytes[2048 + 1024 + 104..2048 + 1024 + 120].copy_from_slice(&[
        0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ]);
    bytes
}

#[test]
fn gpt_unique_uuid_survives_probe_and_source_partition_storage() {
    let unique = [
        0x33, 0x22, 0x11, 0, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
    ];
    let probe = detect_image_filesystem(&mut Cursor::new(image(unique))).unwrap();
    let partition = &probe.partitions[0];
    assert_eq!(
        probe.gpt_disk_guid.as_deref(),
        Some("12345678-9ABC-DEF0-1122-334455667788")
    );
    assert_eq!(
        partition.gpt_partition_uuid.as_deref(),
        Some("00112233-4455-6677-8899-AABBCCDDEEFF")
    );
    assert_ne!(partition.gpt_partition_uuid, partition.type_guid);
    assert_eq!(
        partition.filesystem_uuid.as_deref(),
        Some("10325476-98ba-dcfe-1122-334455667788")
    );
    assert_ne!(partition.filesystem_uuid, partition.gpt_partition_uuid);
    assert_ne!(partition.filesystem_uuid, probe.gpt_disk_guid);
    let connection = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&connection).unwrap();
    let version_before = persistence_sqlite::runner::current_version(&connection).unwrap();
    file_service::store_data_source_partitions_with_gpt_disk_guid(
        &connection,
        &DataSourceId("source".into()),
        &probe.partitions,
        probe.gpt_disk_guid.as_deref(),
    )
    .unwrap();
    let identities = PartitionRepo::new(&connection)
        .identities_for_data_source("source")
        .unwrap();
    assert_eq!(identities.gpt_disk_guid, probe.gpt_disk_guid);
    assert_eq!(
        identities.partitions[&1].filesystem_uuid,
        partition.filesystem_uuid
    );
    assert_eq!(
        identities.partitions[&1].gpt_partition_uuid.as_deref(),
        partition.gpt_partition_uuid.as_deref()
    );
    assert_eq!(
        persistence_sqlite::runner::current_version(&connection).unwrap(),
        version_before
    );
}

#[test]
fn nil_gpt_identity_remains_unrecorded() {
    let probe = detect_image_filesystem(&mut Cursor::new(image([0; 16]))).unwrap();
    assert!(probe.partitions[0].gpt_partition_uuid.is_none());
    assert!(probe.partitions[0].type_guid.is_some());
}

#[test]
fn direct_ext4_probe_records_filesystem_uuid_separately() {
    let mut bytes = vec![0u8; 8192];
    bytes[1024 + 0x38..1024 + 0x3a].copy_from_slice(&0xEF53u16.to_le_bytes());
    let filesystem_uuid = [
        0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ];
    bytes[1024 + 104..1024 + 120].copy_from_slice(&filesystem_uuid);
    let probe = detect_image_filesystem(&mut Cursor::new(bytes)).unwrap();
    assert_eq!(probe.partitions.len(), 1);
    assert_eq!(
        probe.partitions[0].filesystem_uuid.as_deref(),
        Some("10325476-98ba-dcfe-1122-334455667788")
    );
    assert!(probe.partitions[0].gpt_partition_uuid.is_none());
}

#[test]
fn xfs_uuid_uses_filesystem_identity_instead_of_metadata_uuid() {
    let mut bytes = vec![0u8; 8192];
    bytes[..4].copy_from_slice(b"XFSB");
    bytes[32..48].copy_from_slice(&[0x11; 16]);
    bytes[248..264].copy_from_slice(&[0x22; 16]);
    let probe = detect_image_filesystem(&mut Cursor::new(bytes)).unwrap();
    assert_eq!(
        probe.partitions[0].filesystem_uuid.as_deref(),
        Some("11111111-1111-1111-1111-111111111111")
    );
    assert!(probe.partitions[0].gpt_partition_uuid.is_none());
}

#[test]
fn btrfs_uuid_is_the_superblock_fsid_and_nil_uuid_is_absent() {
    for (raw, expected) in [
        ([0x33; 16], Some("33333333-3333-3333-3333-333333333333")),
        ([0; 16], None),
    ] {
        let mut bytes = vec![0u8; 128 * 1024];
        bytes[0x10040..0x10048].copy_from_slice(b"_BHRfS_M");
        bytes[0x10020..0x10030].copy_from_slice(&raw);
        let probe = detect_image_filesystem(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(probe.partitions[0].filesystem_uuid.as_deref(), expected);
    }
}

#[test]
fn filesystem_uuid_cannot_be_observed_outside_the_partition_window() {
    let mut bytes = image([0x44; 16]);
    // The ext4 UUID sits beyond this deliberately short GPT partition window.
    bytes[1024 + 40..1024 + 48].copy_from_slice(&4u64.to_le_bytes());
    let probe = detect_image_filesystem(&mut Cursor::new(bytes)).unwrap();
    assert!(probe.partitions[0].filesystem_uuid.is_none());
}
