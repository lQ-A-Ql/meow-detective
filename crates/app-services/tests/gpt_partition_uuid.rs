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
    bytes[2051..2059].copy_from_slice(b"NTFS    ");
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
        partition.uuid.as_deref(),
        Some("00112233-4455-6677-8899-AABBCCDDEEFF")
    );
    assert_ne!(partition.uuid, partition.type_guid);
    let connection = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&connection).unwrap();
    let version_before = persistence_sqlite::runner::current_version(&connection).unwrap();
    file_service::store_data_source_partitions(
        &connection,
        &DataSourceId("source".into()),
        &probe.partitions,
    )
    .unwrap();
    assert_eq!(
        PartitionRepo::new(&connection)
            .uuids_for_data_source("source")
            .unwrap()[&1],
        partition.uuid.as_ref().unwrap().as_str()
    );
    assert_eq!(
        persistence_sqlite::runner::current_version(&connection).unwrap(),
        version_before
    );
}

#[test]
fn nil_gpt_identity_remains_unrecorded() {
    let probe = detect_image_filesystem(&mut Cursor::new(image([0; 16]))).unwrap();
    assert!(probe.partitions[0].uuid.is_none());
    assert!(probe.partitions[0].type_guid.is_some());
}
