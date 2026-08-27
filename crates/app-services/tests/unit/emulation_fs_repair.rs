use super::*;

use domain::{CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use evidence_emulation::{CowDiskConfig, ParentIdentity};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};

const BPS: usize = 512;
const PART_OFFSET: u64 = 1_048_576; // LBA 2048
const FS_BLOCK: usize = 4096;
const FS_BLOCKS: u64 = 512;
const LOG_START_FSB: u64 = 8;
const LOG_BLOCKS: u32 = 256; // 1 MiB log = 2048 basic blocks
const LOG_BB: usize = (LOG_BLOCKS as usize) * FS_BLOCK / BPS;

const SB_MAGIC: usize = 0x00;
const SB_BLOCKSIZE: usize = 0x04;
const SB_DBLOCKS: usize = 0x08;
const SB_UUID: usize = 0x20;
const SB_LOGSTART: usize = 0x30;
const SB_ROOTINO: usize = 0x38;
const SB_AGBLOCKS: usize = 0x54;
const SB_AGCOUNT: usize = 0x58;
const SB_LOGBLOCKS: usize = 0x60;
const SB_VERSIONNUM: usize = 0x64;
const SB_SECTSIZE: usize = 0x66;
const SB_INODESIZE: usize = 0x68;
const SB_INOPBLOCK: usize = 0x6A;
const SB_LOGSECTSIZE: usize = 0xC2;

/// One-partition MBR disk with a v5 XFS volume whose log holds an ordinary
/// (non-unmount) transaction record at the head — the dirty-boot case.
fn dirty_xfs_disk() -> Vec<u8> {
    let mut disk = vec![0u8; PART_OFFSET as usize + FS_BLOCKS as usize * FS_BLOCK];
    disk[510] = 0x55;
    disk[511] = 0xAA;
    disk[446 + 4] = 0x83;
    disk[446 + 8..446 + 12].copy_from_slice(&2048u32.to_le_bytes());
    disk[446 + 12..446 + 16]
        .copy_from_slice(&((FS_BLOCKS as usize * FS_BLOCK / BPS) as u32).to_le_bytes());

    let sb = &mut disk[PART_OFFSET as usize..PART_OFFSET as usize + BPS];
    sb[SB_MAGIC..SB_MAGIC + 4].copy_from_slice(&0x5846_5342u32.to_be_bytes());
    sb[SB_BLOCKSIZE..SB_BLOCKSIZE + 4].copy_from_slice(&(FS_BLOCK as u32).to_be_bytes());
    sb[SB_DBLOCKS..SB_DBLOCKS + 8].copy_from_slice(&FS_BLOCKS.to_be_bytes());
    sb[SB_UUID..SB_UUID + 16].copy_from_slice(&[0x42u8; 16]);
    sb[SB_LOGSTART..SB_LOGSTART + 8].copy_from_slice(&LOG_START_FSB.to_be_bytes());
    sb[SB_ROOTINO..SB_ROOTINO + 8].copy_from_slice(&128u64.to_be_bytes());
    sb[0x40..0x48].copy_from_slice(&129u64.to_be_bytes()); // rbmino
    sb[0x48..0x50].copy_from_slice(&130u64.to_be_bytes()); // rsumino
    sb[0x7B] = 3; // inopblog: 8 inodes per 4 KiB block
    sb[0x7C] = 6; // agblklog: ceil(log2(40))
    sb[SB_AGBLOCKS..SB_AGBLOCKS + 4].copy_from_slice(&(FS_BLOCKS as u32).to_be_bytes());
    sb[SB_AGCOUNT..SB_AGCOUNT + 4].copy_from_slice(&1u32.to_be_bytes());
    sb[SB_LOGBLOCKS..SB_LOGBLOCKS + 4].copy_from_slice(&LOG_BLOCKS.to_be_bytes());
    sb[SB_VERSIONNUM..SB_VERSIONNUM + 2].copy_from_slice(&5u16.to_be_bytes());
    sb[SB_SECTSIZE..SB_SECTSIZE + 2].copy_from_slice(&(BPS as u16).to_be_bytes());
    sb[SB_INODESIZE..SB_INODESIZE + 2].copy_from_slice(&512u16.to_be_bytes());
    sb[SB_INOPBLOCK..SB_INOPBLOCK + 2].copy_from_slice(&8u16.to_be_bytes());
    sb[SB_LOGSECTSIZE..SB_LOGSECTSIZE + 2].copy_from_slice(&(BPS as u16).to_be_bytes());

    let log_start = PART_OFFSET as usize + LOG_START_FSB as usize * FS_BLOCK;
    let log = &mut disk[log_start..log_start + LOG_BLOCKS as usize * FS_BLOCK];
    for block in 0..LOG_BB {
        let cycle: u32 = if block < 4 { 6 } else { 5 };
        log[block * BPS..block * BPS + 4].copy_from_slice(&cycle.to_be_bytes());
    }
    log[0..4].copy_from_slice(&0xFEED_BABEu32.to_be_bytes());
    log[4..8].copy_from_slice(&6u32.to_be_bytes());
    log[8..12].copy_from_slice(&2u32.to_be_bytes());
    log[12..16].copy_from_slice(&(BPS as u32).to_be_bytes());
    let lsn = 6u64 << 32;
    log[16..24].copy_from_slice(&lsn.to_be_bytes());
    log[24..32].copy_from_slice(&lsn.to_be_bytes());
    log[36..40].copy_from_slice(&u32::MAX.to_be_bytes());
    log[40..44].copy_from_slice(&1u32.to_be_bytes());
    log[44..48].copy_from_slice(&1u32.to_be_bytes());
    log[300..304].copy_from_slice(&1u32.to_be_bytes());
    log[304..320].copy_from_slice(&[0x42u8; 16]);
    log[320..324].copy_from_slice(&(32 * 1024u32).to_be_bytes());
    let body = BPS;
    log[body..body + 4].copy_from_slice(&6u32.to_be_bytes());
    log[body + 8] = 0x69;
    let mut crc = ceph_wire::crc32c::crc32c(u32::MAX, &log[..32]);
    crc = ceph_wire::crc32c::crc32c(crc, &[0u8; 4]);
    crc = ceph_wire::crc32c::crc32c(crc, &log[36..328]);
    crc = ceph_wire::crc32c::crc32c(crc, &log[body..body + BPS]);
    log[32..36].copy_from_slice(&(!crc).to_le_bytes());
    disk
}

struct RepairFixture {
    _temp: tempfile::TempDir,
    case_root: std::path::PathBuf,
    case_id: CaseId,
    data_source_id: DataSourceId,
    conn: rusqlite::Connection,
    image_path: std::path::PathBuf,
}

fn setup(image: &[u8]) -> RepairFixture {
    let temp = tempfile::TempDir::new().unwrap();
    let case_root = temp.path().join("case");
    std::fs::create_dir_all(&case_root).unwrap();
    let image_path = temp.path().join("source.raw");
    std::fs::write(&image_path, image).unwrap();

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case_id = CaseId("case-fs-repair".to_string());
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "fs repair".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .unwrap();
    let data_source_id = DataSourceId("ds-xfs".to_string());
    let source = DataSource {
        id: data_source_id.clone(),
        name: "ds-xfs".to_string(),
        kind: DataSourceKind::Raw,
        source_path: image_path.clone(),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db("ds-xfs", Some("linux"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(&conn)
        .insert_with_storage(&case_id, &source, &storage)
        .unwrap();
    let source_path = crate::source_db::source_db_path(&case_root, &data_source_id);
    std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    let source_conn = persistence_sqlite::open_or_create_source(&source_path).unwrap();
    PartitionRepo::new(&source_conn)
        .insert_batch(&[DataSourcePartitionRecord {
            id: "ds-xfs:partition:0".to_string(),
            data_source_id: "ds-xfs".to_string(),
            partition_index: 0,
            name: "root".to_string(),
            kind_label: "XFS".to_string(),
            status: "supported".to_string(),
            type_guid: None,
            offset: PART_OFFSET,
            length: FS_BLOCKS * FS_BLOCK as u64,
            filesystem: Some("XFS".to_string()),
            unlock_hint: None,
            lvm_vg_uuid: None,
            lvm_vg_name: None,
            lvm_lv_uuid: None,
            lvm_lv_name: None,
            lvm_pv_offsets_json: None,
            lvm_pv_sources_json: None,
        }])
        .unwrap();
    drop(source_conn);

    RepairFixture {
        case_root,
        case_id,
        data_source_id,
        conn,
        image_path,
        _temp: temp,
    }
}

fn session_disk(fixture: &RepairFixture) -> Arc<CowDisk> {
    let provider = evidence_block::open_block_provider(
        &fixture.image_path,
        evidence_block::EvidenceImageKind::Raw,
    )
    .unwrap();
    let identity = ParentIdentity::new(provider.len(), [3u8; 32]).unwrap();
    Arc::new(
        CowDisk::create(
            &fixture.case_root.join("overlay.cow"),
            provider,
            identity,
            CowDiskConfig::default(),
        )
        .unwrap(),
    )
}

fn context(fixture: &RepairFixture) -> BypassCaseContext<'_> {
    BypassCaseContext {
        case_conn: &fixture.conn,
        case_root: &fixture.case_root,
        case_id: &fixture.case_id,
        data_source_id: &fixture.data_source_id,
    }
}

#[test]
fn dirty_log_is_cleared_and_mount_metadata_normalized() {
    let image = dirty_xfs_disk();
    let fixture = setup(&image);
    let disk = session_disk(&fixture);

    let result = repair_xfs_logs(&disk, &context(&fixture)).unwrap();
    assert_eq!(result.items.len(), 1);
    let item = &result.items[0];
    assert_eq!(item.partition_index, 0);
    assert_eq!(item.initial_state, EmulationFsVolumeStateDto::Dirty);
    assert_eq!(item.state, EmulationFsVolumeStateDto::Clean);
    assert!(item.repaired);
    assert_eq!(item.log_bytes, u64::from(LOG_BLOCKS) * FS_BLOCK as u64);

    // This synthetic record carries no replayable metadata, so only the log
    // area changes. Real dirty logs may also materialize committed metadata.
    let mut sb = [0u8; 512];
    disk.read_exact_at(PART_OFFSET, &mut sb).unwrap();
    assert_eq!(
        &sb,
        &image[PART_OFFSET as usize..PART_OFFSET as usize + BPS]
    );

    // The evidence image is byte-identical.
    assert_eq!(std::fs::read(&fixture.image_path).unwrap(), image);

    // A second run sees a clean log and writes nothing.
    let second = repair_xfs_logs(&disk, &context(&fixture)).unwrap();
    assert_eq!(
        second.items[0].initial_state,
        EmulationFsVolumeStateDto::Clean
    );
    assert_eq!(second.items[0].state, EmulationFsVolumeStateDto::Clean);
    assert!(!second.items[0].repaired);
}

#[test]
fn unsupported_second_volume_prevents_writes_to_the_first() {
    let image = dirty_xfs_disk();
    let fixture = setup(&image);
    let source_path = crate::source_db::source_db_path(&fixture.case_root, &fixture.data_source_id);
    let source_conn = persistence_sqlite::open_or_create_source(&source_path).unwrap();
    PartitionRepo::new(&source_conn)
        .insert_batch(&[DataSourcePartitionRecord {
            id: "ds-xfs:partition:1".to_string(),
            data_source_id: "ds-xfs".to_string(),
            partition_index: 1,
            name: "unsupported".to_string(),
            kind_label: "XFS".to_string(),
            status: "supported".to_string(),
            type_guid: None,
            offset: PART_OFFSET + FS_BLOCKS * FS_BLOCK as u64 - BPS as u64,
            length: BPS as u64,
            filesystem: Some("XFS".to_string()),
            unlock_hint: None,
            lvm_vg_uuid: None,
            lvm_vg_name: None,
            lvm_lv_uuid: None,
            lvm_lv_name: None,
            lvm_pv_offsets_json: None,
            lvm_pv_sources_json: None,
        }])
        .unwrap();
    drop(source_conn);
    let disk = session_disk(&fixture);
    let log_offset = PART_OFFSET + LOG_START_FSB * FS_BLOCK as u64;
    let mut before = vec![0u8; LOG_BLOCKS as usize * FS_BLOCK];
    disk.read_exact_at(log_offset, &mut before).unwrap();

    let result = repair_xfs_logs(&disk, &context(&fixture)).unwrap();

    assert_eq!(result.items.len(), 2);
    assert_eq!(
        result.items[0].initial_state,
        EmulationFsVolumeStateDto::Dirty
    );
    assert_eq!(result.items[0].state, EmulationFsVolumeStateDto::Dirty);
    assert!(!result.items[0].repaired);
    assert_eq!(
        result.items[1].state,
        EmulationFsVolumeStateDto::Unsupported
    );
    let mut after = vec![0u8; before.len()];
    disk.read_exact_at(log_offset, &mut after).unwrap();
    assert_eq!(
        after, before,
        "planning failure must leave every volume untouched"
    );
}
