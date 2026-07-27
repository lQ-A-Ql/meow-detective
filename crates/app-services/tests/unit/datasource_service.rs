use super::fs_magic::{read_boot_filesystem, SECTOR_SIZE};
use super::lvm::source_identity::lvm_pv_source_key;
use super::*;
use domain::{CaseId, CaseMeta, DataSourceHashStatus, DataSourceKind, DataSourceProvenanceStatus};
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use std::io::{Read, Seek, SeekFrom};
use tempfile::TempDir;
use transport::ServiceErrorCategory;
const SYNTHETIC_PV_SIZE: u64 = 2_097_152;
const SYNTHETIC_PV_OFFSET: u64 = 1_048_576;
const SYNTHETIC_DATA_AREA_START: u64 = 2560;
const SYNTHETIC_PV0_UUID: &str = "00000000000000000000000000000000";
const SYNTHETIC_PV1_UUID: &str = "11111111111111111111111111111111";
fn setup_case() -> (rusqlite::Connection, CaseId) {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case = CaseMeta {
        id: CaseId("case-datasource".to_string()),
        name: "DataSource Test".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    CaseRepo::new(&conn).create(&case).unwrap();
    (conn, case.id)
}
#[test]
fn attach_data_source_records_file_provenance() {
    let tmp = TempDir::new().unwrap();
    let source_path = tmp.path().join("sample.raw");
    std::fs::write(&source_path, b"sample evidence").unwrap();
    let (conn, case_id) = setup_case();
    let attached = attach_data_source(
        &conn,
        &case_id,
        "sample",
        &source_path,
        DataSourceKind::Raw,
        domain::DataSourcePlatform::Windows,
    )
    .unwrap();
    let stored = DataSourceRepo::new(&conn)
        .find_by_case(&case_id)
        .unwrap()
        .into_iter()
        .find(|source| source.id == attached.id)
        .unwrap();
    assert_eq!(stored.provenance.source_hash_sha256, None);
    assert_eq!(stored.provenance.hash_status, DataSourceHashStatus::Pending);
    assert_eq!(stored.provenance.evidence_size, Some(15));
    assert_eq!(stored.provenance.reader_kind.as_deref(), Some("raw"));
    assert_eq!(
        stored.provenance.provenance_status,
        DataSourceProvenanceStatus::Recorded
    );
    assert_eq!(
        stored.provenance.canonical_source_path,
        Some(std::fs::canonicalize(&source_path).unwrap())
    );
    assert!(stored.provenance.warnings.is_empty());
}

#[test]
fn evidence_reader_rejects_ceph_rbd_as_host_image() {
    let result = super::reader::open_evidence_reader(
        std::path::Path::new("C:/path-that-must-not-be-opened"),
        &DataSourceKind::CephRbd,
    );

    let error = match result {
        Ok(_) => panic!("Ceph RBD must not be opened as a host image"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
}

#[test]
fn attach_data_source_records_directory_provenance_without_size() {
    let tmp = TempDir::new().unwrap();
    let source_path = tmp.path().join("logical-evidence");
    std::fs::create_dir(&source_path).unwrap();
    let (conn, case_id) = setup_case();
    let attached = attach_data_source(
        &conn,
        &case_id,
        "logical",
        &source_path,
        DataSourceKind::LogicalDirectory,
        domain::DataSourcePlatform::Windows,
    )
    .unwrap();
    let stored = DataSourceRepo::new(&conn)
        .find_by_case(&case_id)
        .unwrap()
        .into_iter()
        .find(|source| source.id == attached.id)
        .unwrap();

    assert_eq!(
        stored.provenance.hash_status,
        DataSourceHashStatus::Unavailable
    );
    assert_eq!(stored.provenance.evidence_size, None);
    assert_eq!(
        stored.provenance.reader_kind.as_deref(),
        Some("logical_directory")
    );
    assert_eq!(
        stored.provenance.provenance_status,
        DataSourceProvenanceStatus::Recorded
    );
    assert_eq!(
        stored.provenance.canonical_source_path,
        Some(std::fs::canonicalize(&source_path).unwrap())
    );
    assert!(stored.provenance.warnings.is_empty());
}

#[test]
fn read_boot_filesystem_detects_ext4_magic_inside_superblock() {
    let mut image = vec![0u8; 4096];
    image[1024 + 0x38..1024 + 0x3A].copy_from_slice(&0xEF53u16.to_le_bytes());

    let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

    assert_eq!(detected, Some(ImageFilesystemKind::Ext4));
}

#[test]
fn read_boot_filesystem_detects_btrfs_magic_inside_superblock() {
    let mut image = vec![0u8; 0x11000];
    image[0x10000 + 0x40..0x10000 + 0x48].copy_from_slice(b"_BHRfS_M");

    let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

    assert_eq!(detected, Some(ImageFilesystemKind::Btrfs));
}

#[test]
fn bluestore_detector_recognizes_official_primary_label() {
    let mut image = vec![0u8; 4096];
    image[..b"bluestore block device".len()].copy_from_slice(b"bluestore block device");

    assert!(has_bluestore_label(&mut std::io::Cursor::new(image)).unwrap());
}

#[test]
fn bluestore_detector_rejects_non_matching_media() {
    let mut image = std::io::Cursor::new(vec![0u8; 4096]);

    assert!(!has_bluestore_label(&mut image).unwrap());
}

#[test]
fn bluestore_detector_checks_all_official_label_offsets() {
    const OFFSETS: [u64; 5] = [
        0,
        1024 * 1024 * 1024,
        10 * 1024 * 1024 * 1024,
        100 * 1024 * 1024 * 1024,
        1000 * 1024 * 1024 * 1024,
    ];

    for offset in OFFSETS {
        let mut reader = SparseBlueStoreReader::new(offset);
        assert!(
            has_bluestore_label(&mut reader).unwrap(),
            "BlueStore label at offset {offset} was not detected"
        );
    }
}

#[test]
fn bluestore_detector_supports_partition_relative_labels() {
    let partition_offset = 1024 * 1024;
    let label_offset = partition_offset + 1024 * 1024 * 1024;
    let mut reader = SparseBlueStoreReader::new(label_offset);

    assert!(bluestore::has_bluestore_label_at(&mut reader, partition_offset).unwrap());
}

#[test]
fn bluestore_error_exposes_stable_unsupported_contract() {
    let error = DataSourceError::UnsupportedCephBlueStore;

    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Unsupported
    ));
    assert_eq!(error.code(), Some("CEPH_BLUESTORE_UNSUPPORTED"));
    assert_eq!(error.recoverable(), Some(false));
    assert_eq!(
        error
            .safe_details()
            .and_then(|details| details["format"].as_str().map(str::to_string))
            .as_deref(),
        Some("cephBlueStore")
    );
}

#[test]
fn read_boot_filesystem_prefers_lvm_over_stale_xfs_magic() {
    let mut image = vec![0u8; 4096];
    let image_len = image.len() as u64;
    image[0..4].copy_from_slice(b"XFSB");
    let sector = &mut image[512..1024];
    sector[0..8].copy_from_slice(b"LABELONE");
    sector[8..16].copy_from_slice(&1u64.to_le_bytes());
    sector[20..24].copy_from_slice(&32u32.to_le_bytes());
    sector[24..32].copy_from_slice(b"LVM2 001");
    sector[32..64].copy_from_slice(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    sector[64..72].copy_from_slice(&image_len.to_le_bytes());
    sector[72..80].copy_from_slice(&2048u64.to_le_bytes());
    sector[80..88].copy_from_slice(&(image_len - 2048).to_le_bytes());
    let crc = fs_lvm::crc::lvm_crc32(&sector[20..512]);
    sector[16..20].copy_from_slice(&crc.to_le_bytes());

    let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

    assert_eq!(detected, Some(ImageFilesystemKind::LvmPool));
}

#[test]
fn detect_image_filesystem_detects_raw_lvm_pv_at_offset_zero() {
    let (pv0, _pv1) = build_synthetic_multi_pv_lvm_disks();
    let mut reader = std::io::Cursor::new(pv0);

    let probe = detect_image_filesystem(&mut reader).unwrap();

    assert_eq!(probe.candidates.len(), 1);
    assert_eq!(probe.candidates[0].kind, ImageFilesystemKind::LvmPool);
    assert_eq!(probe.candidates[0].offset, 0);
    assert_eq!(
        probe.partitions[0].status,
        PartitionStatus::Supported,
        "raw whole-disk LVM PVs must be retained for later LV expansion"
    );
}

struct SparseBlueStoreReader {
    position: u64,
    label_offset: u64,
}

impl SparseBlueStoreReader {
    fn new(label_offset: u64) -> Self {
        Self {
            position: 0,
            label_offset,
        }
    }
}

impl Read for SparseBlueStoreReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let signature = b"bluestore block device";
        if self.position == self.label_offset {
            let length = buffer.len().min(signature.len());
            buffer[..length].copy_from_slice(&signature[..length]);
            self.position += length as u64;
            return Ok(length);
        }
        Ok(0)
    }
}

impl Seek for SparseBlueStoreReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(position) => position,
            SeekFrom::Current(delta) => {
                self.position.checked_add_signed(delta).ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek overflow")
                })?
            }
            SeekFrom::End(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "end-relative seek is not supported",
                ))
            }
        };
        Ok(self.position)
    }
}

#[test]
fn expand_lvm_pool_candidates_with_sources_groups_extra_pvs() {
    let tmp = TempDir::new().unwrap();
    let primary_path = tmp.path().join("pv0.raw");
    let extra_path = tmp.path().join("pv1.raw");
    let (mut pv0, pv1) = build_synthetic_multi_pv_lvm_disks();
    let data_area_start = SYNTHETIC_DATA_AREA_START as usize;
    pv0[data_area_start..data_area_start + 4].copy_from_slice(b"XFSB");
    std::fs::write(&primary_path, build_synthetic_lvm_mbr_image(&pv0)).unwrap();
    std::fs::write(&extra_path, build_synthetic_lvm_mbr_image(&pv1)).unwrap();

    let mut reader = evidence_core::RawImageReader::open(&primary_path).unwrap();
    let mut probe = detect_image_filesystem(&mut reader).unwrap();

    expand_lvm_pool_candidates_with_sources(
        &mut probe,
        &primary_path,
        &DataSourceKind::Raw,
        &[LvmDiscoverySource::new(&extra_path, DataSourceKind::Raw)],
    );

    let lvm_candidate = probe
        .candidates
        .iter()
        .find(|candidate| candidate.source == ImageFilesystemSource::LvmLogicalVolume)
        .unwrap_or_else(|| {
            panic!(
                "expected expanded LVM logical volume candidate; candidates={:?}; warnings={:?}",
                probe.candidates, probe.warnings
            )
        });
    assert_eq!(lvm_candidate.kind, ImageFilesystemKind::Xfs);
    let identity = lvm_candidate.lvm_identity.as_ref().unwrap();
    assert_eq!(identity.vg_name, "test_vg");
    assert_eq!(identity.lv_name, "root");
    assert_eq!(identity.pv_sources.len(), 2);
    assert_eq!(
        identity.pv_sources[0].source_path,
        primary_path.display().to_string()
    );
    assert_eq!(
        identity.pv_sources[0].source_kind,
        Some(DataSourceKind::Raw)
    );
    assert_eq!(identity.pv_sources[0].pv_uuid, SYNTHETIC_PV0_UUID);
    assert_eq!(identity.pv_sources[0].pv_name.as_deref(), Some("pv0"));
    assert_eq!(
        identity.pv_sources[1].source_path,
        extra_path.display().to_string()
    );
    assert_eq!(
        identity.pv_sources[1].source_kind,
        Some(DataSourceKind::Raw)
    );
    assert_eq!(identity.pv_sources[1].pv_uuid, SYNTHETIC_PV1_UUID);
    assert_eq!(identity.pv_sources[1].pv_name.as_deref(), Some("pv1"));
    assert!(probe
        .partitions
        .iter()
        .any(|partition| partition.status == PartitionStatus::Expanded));
    assert_eq!(
        probe
            .partitions
            .iter()
            .filter(|partition| partition.status == PartitionStatus::Expanded)
            .count(),
        1
    );
    assert_eq!(
        probe
            .partitions
            .iter()
            .find(|partition| partition.lvm_identity.is_some())
            .and_then(|partition| partition.lvm_identity.as_ref())
            .unwrap()
            .pv_sources,
        identity.pv_sources
    );
}

#[test]
fn expand_lvm_pool_candidates_keeps_legacy_single_source_api() {
    let tmp = TempDir::new().unwrap();
    let primary_path = tmp.path().join("pv0.raw");
    let (mut pv0, _pv1) = build_synthetic_multi_pv_lvm_disks();
    let data_area_start = SYNTHETIC_DATA_AREA_START as usize;
    pv0[data_area_start..data_area_start + 4].copy_from_slice(b"XFSB");
    std::fs::write(&primary_path, build_synthetic_lvm_mbr_image(&pv0)).unwrap();

    let mut reader = evidence_core::RawImageReader::open(&primary_path).unwrap();
    let mut probe = detect_image_filesystem(&mut reader).unwrap();

    expand_lvm_pool_candidates(&mut probe, &primary_path, &DataSourceKind::Raw);

    assert!(probe
        .candidates
        .iter()
        .all(|candidate| candidate.source != ImageFilesystemSource::LvmLogicalVolume));
    assert!(
        probe
            .warnings
            .iter()
            .any(|warning| warning.contains("skipping incomplete")),
        "expected incomplete LVM warning, got {:?}",
        probe.warnings
    );
}

#[test]
fn lvm_pv_source_key_includes_source_path_and_uuid() {
    let left = LvmPhysicalVolumeSource {
        source_path: "disk-a.E01".to_string(),
        source_kind: Some(DataSourceKind::E01),
        offset: 1_048_576,
        pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        pv_name: Some("pv0".to_string()),
    };
    let same_offset_different_source = LvmPhysicalVolumeSource {
        source_path: "disk-b.E01".to_string(),
        source_kind: Some(DataSourceKind::E01),
        offset: 1_048_576,
        pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        pv_name: Some("pv0".to_string()),
    };
    let same_offset_different_uuid = LvmPhysicalVolumeSource {
        source_path: "disk-a.E01".to_string(),
        source_kind: Some(DataSourceKind::E01),
        offset: 1_048_576,
        pv_uuid: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        pv_name: Some("pv1".to_string()),
    };

    assert_ne!(
        lvm_pv_source_key(&left),
        lvm_pv_source_key(&same_offset_different_source)
    );
    assert_ne!(
        lvm_pv_source_key(&left),
        lvm_pv_source_key(&same_offset_different_uuid)
    );
}

#[test]
fn lvm_pv_source_key_includes_source_kind() {
    let e01 = LvmPhysicalVolumeSource {
        source_path: "disk.E01".to_string(),
        source_kind: Some(DataSourceKind::E01),
        offset: 0,
        pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        pv_name: Some("pv0".to_string()),
    };
    let raw = LvmPhysicalVolumeSource {
        source_path: "disk.E01".to_string(),
        source_kind: Some(DataSourceKind::Raw),
        offset: 0,
        pv_uuid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        pv_name: Some("pv0".to_string()),
    };

    assert_ne!(lvm_pv_source_key(&e01), lvm_pv_source_key(&raw));
}

fn build_synthetic_multi_pv_lvm_disks() -> (Vec<u8>, Vec<u8>) {
    let metadata_text = format!(
        r#"test_vg {{
    id = "vg-multi-pv-1234"
    seqno = 2
    extent_size = 1

    physical_volumes {{
        pv0 {{
            id = "{}"
            device = "/dev/sda1"
            pe_start = 5
            pe_count = 16
        }}
        pv1 {{
            id = "{}"
            device = "/dev/sdb1"
            pe_start = 5
            pe_count = 16
        }}
    }}

    logical_volumes {{
        root {{
            id = "lv-root-uuid"
            status = ["READ","WRITE","VISIBLE"]
            segment_count = 1
            segment1 {{
                start_extent = 0
                extent_count = 1
                type = "linear"
                stripe_count = 1
                stripes = ["pv0", 0]
            }}
        }}
    }}
}}
"#,
        SYNTHETIC_PV0_UUID, SYNTHETIC_PV1_UUID
    );

    let mut pv0 = vec![0u8; SYNTHETIC_PV_SIZE as usize];
    let mut pv1 = vec![0u8; SYNTHETIC_PV_SIZE as usize];
    write_synthetic_lvm_pv_label(&mut pv0, SYNTHETIC_PV0_UUID);
    write_synthetic_lvm_pv_label(&mut pv1, SYNTHETIC_PV1_UUID);
    write_synthetic_lvm_metadata(&mut pv0, &metadata_text);
    (pv0, pv1)
}

fn build_synthetic_lvm_mbr_image(pv: &[u8]) -> Vec<u8> {
    let image_len = SYNTHETIC_PV_OFFSET as usize + pv.len();
    let mut image = vec![0u8; image_len];
    image[SYNTHETIC_PV_OFFSET as usize..].copy_from_slice(pv);
    write_synthetic_mbr_partition(&mut image, SYNTHETIC_PV_OFFSET / SECTOR_SIZE, pv.len());
    image
}

fn write_synthetic_mbr_partition(image: &mut [u8], lba_start: u64, byte_len: usize) {
    let sector_count = (byte_len as u64 / SECTOR_SIZE) as u32;
    let lba_start = lba_start as u32;
    let entry = &mut image[446..462];
    entry[4] = 0x8e;
    entry[8..12].copy_from_slice(&lba_start.to_le_bytes());
    entry[12..16].copy_from_slice(&sector_count.to_le_bytes());
    image[510] = 0x55;
    image[511] = 0xaa;
}

fn write_synthetic_lvm_pv_label(disk: &mut [u8], pv_uuid: &str) {
    let pv_size = disk.len() as u64;
    let sec = &mut disk[512..1024];
    sec[0..8].copy_from_slice(b"LABELONE");
    sec[8..16].copy_from_slice(&1u64.to_le_bytes());
    sec[20..24].copy_from_slice(&32u32.to_le_bytes());
    sec[24..32].copy_from_slice(b"LVM2 001");
    sec[32..64].copy_from_slice(format!("{pv_uuid:32}").as_bytes());
    sec[64..72].copy_from_slice(&pv_size.to_le_bytes());
    sec[72..80].copy_from_slice(&SYNTHETIC_DATA_AREA_START.to_le_bytes());
    sec[80..88].copy_from_slice(&(pv_size - SYNTHETIC_DATA_AREA_START).to_le_bytes());
    sec[104..112].copy_from_slice(&1024u64.to_le_bytes());
    sec[112..120].copy_from_slice(&(4 * 512u64).to_le_bytes());
    let crc = fs_lvm::crc::lvm_crc32(&sec[20..512]);
    sec[16..20].copy_from_slice(&crc.to_le_bytes());
}

fn write_synthetic_lvm_metadata(disk: &mut [u8], metadata_text: &str) {
    let text_bytes = metadata_text.as_bytes();
    let text_offset = 1536usize;
    let text_end = text_offset + text_bytes.len();
    assert!(text_end <= disk.len());

    {
        let mda = &mut disk[1024..1536];
        mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
        mda[20..24].copy_from_slice(&1u32.to_le_bytes());
        mda[24..32].copy_from_slice(&1024u64.to_le_bytes());
        mda[32..40].copy_from_slice(&1536u64.to_le_bytes());
        mda[40..48].copy_from_slice(&512u64.to_le_bytes());
    }

    disk[text_offset..text_end].copy_from_slice(text_bytes);

    let text_size = text_bytes.len() as u64;
    let text_crc = fs_lvm::crc::lvm_crc32(text_bytes);
    {
        let mda = &mut disk[1024..1536];
        mda[48..56].copy_from_slice(&text_size.to_le_bytes());
        mda[56..60].copy_from_slice(&text_crc.to_le_bytes());
        let mda_crc = fs_lvm::crc::lvm_crc32(&mda[4..512]);
        mda[0..4].copy_from_slice(&mda_crc.to_le_bytes());
    }
}

fn candidate(offset: u64, partition_index: Option<usize>) -> ImageFilesystemCandidate {
    ImageFilesystemCandidate {
        partition_index,
        partition_name: None,
        kind: ImageFilesystemKind::Ntfs,
        offset,
        length: None,
        source: ImageFilesystemSource::MbrPartition,
        lvm_identity: None,
    }
}

#[test]
fn assign_indices_sorts_by_offset() {
    let candidates = vec![
        candidate(3000, None),
        candidate(1000, None),
        candidate(2000, None),
    ];
    let map = assign_effective_partition_indices(&candidates);
    // sorted by offset: 1000 -> idx0, 2000 -> idx1, 3000 -> idx2
    assert_eq!(effective_partition_index(&candidates[0], 0, &map), 2);
    assert_eq!(effective_partition_index(&candidates[1], 1, &map), 0);
    assert_eq!(effective_partition_index(&candidates[2], 2, &map), 1);
}

#[test]
fn assign_indices_preserves_existing() {
    let candidates = vec![
        candidate(2000, Some(5)),
        candidate(1000, None),
        candidate(3000, None),
    ];
    let map = assign_effective_partition_indices(&candidates);
    // existing index preserved
    assert_eq!(effective_partition_index(&candidates[0], 0, &map), 5);
    // sorted by offset: 1000 -> idx0, 3000 -> idx1
    assert_eq!(effective_partition_index(&candidates[1], 1, &map), 0);
    assert_eq!(effective_partition_index(&candidates[2], 2, &map), 1);
}

#[test]
fn assign_indices_single_candidate() {
    let candidates = vec![candidate(500, None)];
    let map = assign_effective_partition_indices(&candidates);
    assert_eq!(effective_partition_index(&candidates[0], 0, &map), 0);
}

#[test]
fn assign_indices_empty() {
    let candidates: Vec<ImageFilesystemCandidate> = vec![];
    let map = assign_effective_partition_indices(&candidates);
    assert!(map.is_empty());
}
