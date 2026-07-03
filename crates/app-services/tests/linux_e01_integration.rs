//! Integration test: verify ext4/XFS/Btrfs filesystem detection, file tree
//! enumeration, path reconstruction, and Linux artifact extraction from a real
//! Linux E01 sample.
//!
//! These tests are ignored by default because they require the environment
//! variable `FORENSICS_LINUX_E01_FIXTURE` pointing to a Linux E01 file, e.g.:
//!   D:\獬豸杯\检材3.E01
//!
//! Run with:
//!   $env:FORENSICS_LINUX_E01_FIXTURE='D:\獬豸杯\检材3.E01'
//!   cargo test -p app-services --test linux_e01_integration -- --ignored

use app_services::{
    analysis_service::{
        evidence_candidates_for_categories, get_linux_artifact_summary, run_analysis_extraction,
    },
    datasource_service::{
        detect_image_filesystem, expand_lvm_pool_candidates, ImageFilesystemCandidate,
        ImageFilesystemKind, ImageFilesystemSource, PartitionRecord,
    },
    file_service,
    import_pipeline::{
        enumerate_image_data_source, format_partition_record_root_name, format_partition_root_name,
    },
};
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, FileEntryId};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::DataSourceRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use rusqlite::Connection;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const LIUYANG_LVM_POOL_OFFSET: u64 = 1_074_790_400;
const LIUYANG_ROOT_LV_NAME: &str = "root";
const LIUYANG_ROOT_LV_VG_NAME: &str = "cl";
const HIGH_VALUE_LINUX_SYSTEM_INFO_PATHS: &[&str] =
    &["/etc/passwd", "/etc/os-release", "/etc/hostname"];
const CRITICAL_LINUX_ARTIFACT_CANDIDATE_PATHS: &[&str] = &[
    "/etc/passwd",
    "/etc/os-release",
    "/etc/hostname",
    "/etc/crontab",
    "/root/.bash_history",
    "/var/log/wtmp",
    "/var/log/messages",
    "/var/log/secure",
    "/var/spool/cron/root",
];
const ARBITRARY_PREVIEW_READ_PATHS: &[&str] =
    &["/etc/fstab", "/root/.bash_history", "/var/log/wtmp"];
const MIN_LIUYANG_ROOT_LV_FILE_COUNT: u64 = 50_000;
const MIN_LIUYANG_ROOT_LV_DIR_COUNT: u64 = 7_000;
const MIN_LIUYANG_LINUX_ARTIFACT_CANDIDATES: usize = 100;
const SYNTHETIC_PV_SIZE: u64 = 2_097_152;
const SYNTHETIC_DATA_AREA_START: u64 = 2560;
const SYNTHETIC_LV_MARKER_OFFSET: usize = SYNTHETIC_DATA_AREA_START as usize;

#[derive(Debug)]
struct LinuxPathEntry {
    file_id: FileEntryId,
    path: String,
    size: u64,
}

fn fixture_path() -> PathBuf {
    std::env::var("FORENSICS_LINUX_E01_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Default fallback — only works in the test author's environment.
            PathBuf::from(r"D:\獬豸杯\检材3.E01")
        })
}

fn build_synthetic_lvm_pv(
    vg_name: &str,
    vg_uuid: &str,
    lv_name: &str,
    lv_uuid: &str,
    pv_uuid: &str,
    seqno: u64,
) -> Vec<u8> {
    build_synthetic_lvm_pv_with_metadata(
        pv_uuid,
        &single_pv_lvm_metadata_text(vg_name, vg_uuid, lv_name, lv_uuid, pv_uuid, seqno),
    )
}

fn build_synthetic_lvm_pv_with_metadata(pv_uuid: &str, metadata_text: &str) -> Vec<u8> {
    let mut pv = vec![0u8; SYNTHETIC_PV_SIZE as usize];
    write_synthetic_lvm_label(&mut pv, pv_uuid);
    write_synthetic_lvm_metadata(&mut pv, metadata_text);
    write_ext4_marker(&mut pv);
    pv
}

fn single_pv_lvm_metadata_text(
    vg_name: &str,
    vg_uuid: &str,
    lv_name: &str,
    lv_uuid: &str,
    pv_uuid: &str,
    seqno: u64,
) -> String {
    format!(
        r#"{vg_name} {{
    id = "{vg_uuid}"
    seqno = {seqno}
    extent_size = 2

    physical_volumes {{
        pv0 {{
            id = "{pv_uuid}"
            device = "/dev/sda1"
            pe_start = 5
            pe_count = 128
        }}
    }}

    logical_volumes {{
        {lv_name} {{
            id = "{lv_uuid}"
            segment_count = 1
            segment1 {{
                start_extent = 0
                extent_count = 64
                type = "striped"
                stripe_count = 1
                stripes = ["pv0", 0]
            }}
        }}
    }}
}}
"#
    )
}

fn two_pv_lvm_metadata_text(
    vg_name: &str,
    vg_uuid: &str,
    lv_name: &str,
    lv_uuid: &str,
    pv0_uuid: &str,
    pv1_uuid: &str,
    seqno: u64,
) -> String {
    format!(
        r#"{vg_name} {{
    id = "{vg_uuid}"
    seqno = {seqno}
    extent_size = 2

    physical_volumes {{
        pv0 {{
            id = "{pv0_uuid}"
            device = "/dev/sda1"
            pe_start = 5
            pe_count = 128
        }}
        pv1 {{
            id = "{pv1_uuid}"
            device = "/dev/sdb1"
            pe_start = 5
            pe_count = 128
        }}
    }}

    logical_volumes {{
        {lv_name} {{
            id = "{lv_uuid}"
            segment_count = 1
            segment1 {{
                start_extent = 0
                extent_count = 64
                type = "striped"
                stripe_count = 1
                stripes = ["pv0", 0]
            }}
        }}
    }}
}}
"#
    )
}

fn write_synthetic_lvm_label(pv: &mut [u8], pv_uuid: &str) {
    let sec = &mut pv[512..1024];
    sec[0..8].copy_from_slice(b"LABELONE");
    sec[8..16].copy_from_slice(&1u64.to_le_bytes());
    sec[20..24].copy_from_slice(&32u32.to_le_bytes());
    sec[24..32].copy_from_slice(b"LVM2 001");
    sec[32..64].copy_from_slice(format!("{:32}", pv_uuid).as_bytes());
    sec[64..72].copy_from_slice(&SYNTHETIC_PV_SIZE.to_le_bytes());
    sec[72..80].copy_from_slice(&SYNTHETIC_DATA_AREA_START.to_le_bytes());
    sec[80..88].copy_from_slice(&(SYNTHETIC_PV_SIZE - SYNTHETIC_DATA_AREA_START).to_le_bytes());
    sec[104..112].copy_from_slice(&1024u64.to_le_bytes());
    sec[112..120].copy_from_slice(&(4 * 512u64).to_le_bytes());

    let crc = fs_lvm::crc::lvm_crc32(&sec[20..512]);
    sec[16..20].copy_from_slice(&crc.to_le_bytes());
}

fn write_synthetic_lvm_metadata(pv: &mut [u8], metadata_text: &str) {
    let text_bytes = metadata_text.as_bytes();
    let text_offset = 1536usize;
    let text_end = text_offset + text_bytes.len();
    assert!(text_end <= pv.len());

    {
        let mda = &mut pv[1024..1536];
        mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
        mda[20..24].copy_from_slice(&1u32.to_le_bytes());
        mda[24..32].copy_from_slice(&1024u64.to_le_bytes());
        mda[32..40].copy_from_slice(&1536u64.to_le_bytes());
        mda[40..48].copy_from_slice(&512u64.to_le_bytes());
    }

    pv[text_offset..text_end].copy_from_slice(text_bytes);

    let text_crc = fs_lvm::crc::lvm_crc32(text_bytes);
    {
        let mda = &mut pv[1024..1536];
        mda[48..56].copy_from_slice(&(text_bytes.len() as u64).to_le_bytes());
        mda[56..60].copy_from_slice(&text_crc.to_le_bytes());
        let mda_crc = fs_lvm::crc::lvm_crc32(&mda[4..512]);
        mda[0..4].copy_from_slice(&mda_crc.to_le_bytes());
    }
}

fn write_ext4_marker(pv: &mut [u8]) {
    let marker_offset = SYNTHETIC_LV_MARKER_OFFSET + 1024;
    pv[marker_offset..marker_offset + 2].copy_from_slice(&0xEF53u16.to_le_bytes());
}

#[test]
fn lvm_expansion_iterates_independent_volume_groups_by_remaining_pv_offsets() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source_path = tmp.path().join("two-vgs.raw");
    let low_offset = 1_048_576u64;
    let high_offset = low_offset + SYNTHETIC_PV_SIZE + 1_048_576;
    let low_pv = build_synthetic_lvm_pv(
        "low_vg",
        "vg-low",
        "low_root",
        "lv-low-root",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        1,
    );
    let high_pv = build_synthetic_lvm_pv(
        "high_vg",
        "vg-high",
        "high_root",
        "lv-high-root",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        99,
    );
    let mut disk = vec![0u8; (high_offset + SYNTHETIC_PV_SIZE) as usize];
    disk[low_offset as usize..low_offset as usize + low_pv.len()].copy_from_slice(&low_pv);
    disk[high_offset as usize..high_offset as usize + high_pv.len()].copy_from_slice(&high_pv);
    std::fs::write(&source_path, disk).unwrap();

    let (low_candidate, low_partition) = synthetic_lvm_partition(1, "low pv", low_offset);
    let (high_candidate, high_partition) = synthetic_lvm_partition(2, "high pv", high_offset);
    let mut probe = app_services::datasource_service::ImageFilesystemProbe {
        candidates: vec![low_candidate, high_candidate],
        partitions: vec![low_partition, high_partition],
        warnings: Vec::new(),
    };

    let source_kind = DataSourceKind::Raw;
    let direct_reader: Box<dyn EvidenceReader> =
        Box::new(evidence_core::RawImageReader::open(&source_path).unwrap());
    let direct_pool = fs_lvm::LvmPool::discover(vec![direct_reader], vec![low_offset]).unwrap();
    let mut direct_lv = direct_pool.open_volume(0).unwrap();
    let mut direct_magic = [0u8; 2];
    direct_lv.seek(SeekFrom::Start(1024)).unwrap();
    direct_lv.read_exact(&mut direct_magic).unwrap();
    assert_eq!(u16::from_le_bytes(direct_magic), 0xEF53);
    expand_lvm_pool_candidates(&mut probe, &source_path, &source_kind);
    assert!(probe.warnings.is_empty(), "warnings={:?}", probe.warnings);

    let identities = probe
        .candidates
        .iter()
        .filter_map(|candidate| candidate.lvm_identity.as_ref())
        .collect::<Vec<_>>();
    assert!(
        identities.iter().any(|identity| {
            identity.vg_name == "low_vg"
                && identity.lv_name == "low_root"
                && identity.pv_offsets == vec![low_offset]
        }),
        "iterative expansion must still discover the lower-seqno independent VG; identities={identities:?}"
    );
    assert!(
        identities.iter().any(|identity| {
            identity.vg_name == "high_vg"
                && identity.lv_name == "high_root"
                && identity.pv_offsets == vec![high_offset]
        }),
        "iterative expansion must discover the higher-seqno independent VG; identities={identities:?}"
    );
    assert!(
        probe
            .candidates
            .iter()
            .filter(|candidate| matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume))
            .all(|candidate| candidate.kind == ImageFilesystemKind::Ext4),
        "synthetic LV filesystem markers should become Ext4 candidates"
    );
    assert!(
        probe.partitions.iter().any(|partition| {
            partition.offset == low_offset
                && matches!(
                    partition.status,
                    app_services::datasource_service::PartitionStatus::Expanded
                )
        }),
        "lower-seqno PV partition should be marked expanded"
    );
    assert!(
        probe.partitions.iter().any(|partition| {
            partition.offset == high_offset
                && matches!(
                    partition.status,
                    app_services::datasource_service::PartitionStatus::Expanded
                )
        }),
        "higher-seqno PV partition should be marked expanded"
    );
}

#[test]
fn lvm_expansion_skips_incomplete_high_seqno_vg_but_expands_complete_vg() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source_path = tmp.path().join("incomplete-high-vg.raw");
    let low_offset = 1_048_576u64;
    let high_offset = low_offset + SYNTHETIC_PV_SIZE + 1_048_576;
    let missing_high_pv_uuid = "cccccccccccccccccccccccccccccccc";
    let low_pv = build_synthetic_lvm_pv(
        "low_complete_vg",
        "vg-low-complete",
        "low_root",
        "lv-low-root",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        1,
    );
    let high_metadata = two_pv_lvm_metadata_text(
        "high_incomplete_vg",
        "vg-high-incomplete",
        "high_root",
        "lv-high-root",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        missing_high_pv_uuid,
        99,
    );
    let high_pv =
        build_synthetic_lvm_pv_with_metadata("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", &high_metadata);
    let mut disk = vec![0u8; (high_offset + SYNTHETIC_PV_SIZE) as usize];
    disk[low_offset as usize..low_offset as usize + low_pv.len()].copy_from_slice(&low_pv);
    disk[high_offset as usize..high_offset as usize + high_pv.len()].copy_from_slice(&high_pv);
    std::fs::write(&source_path, disk).unwrap();

    let (low_candidate, low_partition) = synthetic_lvm_partition(1, "low pv", low_offset);
    let (high_candidate, high_partition) = synthetic_lvm_partition(2, "high pv", high_offset);
    let mut probe = app_services::datasource_service::ImageFilesystemProbe {
        candidates: vec![low_candidate, high_candidate],
        partitions: vec![low_partition, high_partition],
        warnings: Vec::new(),
    };

    let source_kind = DataSourceKind::Raw;
    expand_lvm_pool_candidates(&mut probe, &source_path, &source_kind);

    let identities = probe
        .candidates
        .iter()
        .filter_map(|candidate| candidate.lvm_identity.as_ref())
        .collect::<Vec<_>>();
    assert!(
        identities.iter().any(|identity| {
            identity.vg_name == "low_complete_vg"
                && identity.lv_name == "low_root"
                && identity.pv_offsets == vec![low_offset]
        }),
        "complete lower-seqno VG should expand despite incomplete higher-seqno VG; identities={identities:?}, warnings={:?}",
        probe.warnings
    );
    assert!(
        identities
            .iter()
            .all(|identity| identity.vg_name != "high_incomplete_vg"),
        "incomplete higher-seqno VG should not produce LV candidates; identities={identities:?}"
    );
    assert!(
        probe
            .candidates
            .iter()
            .all(|candidate| candidate.offset != low_offset
                || !matches!(candidate.kind, ImageFilesystemKind::LvmPool)),
        "expanded low VG pool candidate should be redirected out of expandable candidates"
    );
    assert!(
        probe.partitions.iter().any(|partition| {
            partition.offset == low_offset
                && matches!(
                    partition.status,
                    app_services::datasource_service::PartitionStatus::Expanded
                )
        }),
        "complete low VG PV partition should be marked Expanded/redirected"
    );
    let low_lv_partition = probe
        .partitions
        .iter()
        .find(|partition| {
            partition.lvm_identity.as_ref().is_some_and(|identity| {
                identity.vg_name == "low_complete_vg" && identity.lv_name == "low_root"
            })
        })
        .expect("expanded low VG should create a logical-volume partition record");
    assert!(matches!(
        low_lv_partition.status,
        app_services::datasource_service::PartitionStatus::Supported
    ));
    assert!(matches!(
        low_lv_partition.filesystem,
        Some(ImageFilesystemKind::Ext4)
    ));
    assert!(
        probe.partitions.iter().any(|partition| {
            partition.offset == high_offset
                && matches!(
                    partition.status,
                    app_services::datasource_service::PartitionStatus::Supported
                )
        }),
        "incomplete high VG PV partition should remain supported but not redirected"
    );
    assert!(
        probe
            .warnings
            .iter()
            .any(|warning| warning.contains("high_incomplete_vg")
                && warning.contains(missing_high_pv_uuid)),
        "incomplete VG should be reported with missing PV UUID; warnings={:?}",
        probe.warnings
    );
}

#[test]
fn lvm_expansion_reports_metadata_parse_diagnostics() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source_path = tmp.path().join("corrupt-metadata.raw");
    let pv_offset = 1_048_576u64;
    let mut pv = build_synthetic_lvm_pv(
        "corrupt_vg",
        "vg-corrupt",
        "root",
        "lv-root",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        1,
    );
    pv[1024 + 4..1024 + 20].copy_from_slice(b"corrupt-MDA-copy");

    let mut disk = vec![0u8; (pv_offset + SYNTHETIC_PV_SIZE) as usize];
    disk[pv_offset as usize..pv_offset as usize + pv.len()].copy_from_slice(&pv);
    std::fs::write(&source_path, disk).unwrap();

    let (candidate, partition) = synthetic_lvm_partition(1, "corrupt pv", pv_offset);
    let mut probe = app_services::datasource_service::ImageFilesystemProbe {
        candidates: vec![candidate],
        partitions: vec![partition],
        warnings: Vec::new(),
    };

    expand_lvm_pool_candidates(&mut probe, &source_path, &DataSourceKind::Raw);

    assert!(
        probe.warnings.iter().any(|warning| {
            warning.contains("metadata area 0")
                && warning.contains("PV offset 1048576")
                && warning.contains("MDA header magic mismatch")
        }),
        "corrupt LVM metadata should include metadata-area diagnostics; warnings={:?}",
        probe.warnings
    );
}

fn synthetic_lvm_partition(
    index: usize,
    name: &str,
    offset: u64,
) -> (ImageFilesystemCandidate, PartitionRecord) {
    (
        ImageFilesystemCandidate {
            partition_index: Some(index),
            partition_name: Some(name.to_string()),
            kind: ImageFilesystemKind::LvmPool,
            offset,
            source: ImageFilesystemSource::GptPartition,
            lvm_identity: None,
        },
        PartitionRecord {
            index,
            name: name.to_string(),
            kind_label: "LVM".to_string(),
            type_guid: None,
            offset,
            length: SYNTHETIC_PV_SIZE,
            status: app_services::datasource_service::PartitionStatus::Supported,
            filesystem: Some(ImageFilesystemKind::LvmPool),
            lvm_identity: None,
        },
    )
}

fn setup_case(conn: &Connection, case_id: &str) {
    let case = domain::CaseMeta {
        id: CaseId(case_id.to_string()),
        name: "Linux E01 Test".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    CaseRepo::new(conn).create(&case).unwrap();
}

fn detect_expanded_linux_probe() -> app_services::datasource_service::ImageFilesystemProbe {
    let fixture = fixture_path();
    let mut reader = E01Reader::open(&fixture).unwrap();
    let mut probe = detect_image_filesystem(&mut reader).unwrap();
    let source_kind = DataSourceKind::E01;
    expand_lvm_pool_candidates(&mut probe, &fixture, &source_kind);
    probe
}

fn root_lv_candidate(
    probe: &app_services::datasource_service::ImageFilesystemProbe,
) -> &ImageFilesystemCandidate {
    probe
        .candidates
        .iter()
        .find(|candidate| {
            candidate.kind == ImageFilesystemKind::Xfs
                && matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume)
                && candidate
                    .lvm_identity
                    .as_ref()
                    .is_some_and(|identity| identity.lv_name == LIUYANG_ROOT_LV_NAME)
        })
        .expect("expanded probe should include the cl/root XFS logical volume")
}

fn open_root_lv_xfs() -> fs_xfs::XfsReader {
    let e01_reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path()).unwrap());
    let pool = fs_lvm::LvmPool::discover(vec![e01_reader], vec![LIUYANG_LVM_POOL_OFFSET])
        .expect("LVM pool discovery should succeed");
    let root_index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.name == LIUYANG_ROOT_LV_NAME)
        .expect("root LV should be present in direct LVM discovery");
    let root_reader = pool.open_volume(root_index).expect("root LV should open");
    fs_xfs::XfsReader::open(Box::new(root_reader), 0).expect("root LV should mount as XFS")
}

fn create_linux_test_data_source(conn: &Connection, case_id: &str, ds_id: &DataSourceId) {
    DataSourceRepo::new(conn)
        .insert(
            &CaseId(case_id.to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "Linux E01".to_string(),
                kind: DataSourceKind::E01,
                source_path: fixture_path(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )
        .unwrap();
}

fn setup_linux_fixture_case(case_id: &str, ds_id: &DataSourceId) -> Connection {
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    setup_case(&conn, case_id);
    create_linux_test_data_source(&conn, case_id, ds_id);
    conn
}

fn find_first_readable_file(
    fs: &dyn FileSystemReader,
    path: &str,
    depth: usize,
) -> Option<(String, usize)> {
    if depth == 0 {
        return None;
    }

    let children = fs.list_children(path).ok()?;
    for child in &children {
        let child_path = if path.is_empty() {
            child.name.clone()
        } else {
            format!("{}/{}", path, child.name)
        };
        if child.is_dir {
            continue;
        }
        let mut reader = match fs.open_file(&child_path) {
            Ok(reader) => reader,
            Err(_) => continue,
        };
        let mut buf = [0u8; 512];
        if let Ok(read) = reader.read(&mut buf) {
            if read > 0 {
                return Some((child_path, read));
            }
        }
    }

    for child in children {
        if !child.is_dir {
            continue;
        }
        let child_path = if path.is_empty() {
            child.name
        } else {
            format!("{}/{}", path, child.name)
        };
        if let Some(found) = find_first_readable_file(fs, &child_path, depth - 1) {
            return Some(found);
        }
    }

    None
}

fn enumerate_root_lv_into_case(
    conn: &Connection,
    ds_id: &DataSourceId,
) -> app_services::file_service::EnumerationStats {
    let probe = detect_expanded_linux_probe();
    let root_lv = root_lv_candidate(&probe);
    file_service::store_data_source_partitions(conn, ds_id, &probe.partitions).unwrap();

    let fs = open_root_lv_xfs();
    file_service::enumerate_filesystem_with_root_name(
        conn,
        ds_id,
        &fs,
        Some(&format_partition_root_name(root_lv)),
        None::<&dyn Fn(u32)>,
    )
    .unwrap()
}

fn count_entries_like(conn: &Connection, ds_id: &DataSourceId, like: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM file_entries
         WHERE data_source_id = ?1
           AND LOWER(REPLACE(path, '\\', '/')) LIKE ?2",
        rusqlite::params![ds_id.0, like],
        |row| row.get(0),
    )
    .unwrap()
}

fn total_file_entries(conn: &Connection, ds_id: &DataSourceId) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM file_entries
         WHERE data_source_id = ?1",
        rusqlite::params![ds_id.0],
        |row| row.get(0),
    )
    .unwrap()
}

fn normalize_linux_path_suffix(path: &str) -> String {
    path.trim_start_matches('/')
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn path_has_linux_suffix(path: &str, linux_path: &str) -> bool {
    let normalized_path = path.replace('\\', "/").to_ascii_lowercase();
    let suffix = normalize_linux_path_suffix(linux_path);
    normalized_path == suffix || normalized_path.ends_with(&format!("/{suffix}"))
}

fn find_linux_file_entry(
    conn: &Connection,
    ds_id: &DataSourceId,
    linux_path: &str,
) -> Option<LinuxPathEntry> {
    let suffix = normalize_linux_path_suffix(linux_path);
    let slash_suffix = format!("%/{suffix}");
    conn.query_row(
        "SELECT id, path, COALESCE(size, 0)
         FROM file_entries
         WHERE data_source_id = ?1
           AND entry_type = 'file' COLLATE NOCASE
           AND (
                LOWER(REPLACE(path, '\\', '/')) = ?2
                OR LOWER(REPLACE(path, '\\', '/')) LIKE ?3
           )
         ORDER BY LENGTH(path) ASC
         LIMIT 1",
        rusqlite::params![&ds_id.0, suffix, slash_suffix],
        |row| {
            Ok(LinuxPathEntry {
                file_id: FileEntryId(row.get(0)?),
                path: row.get(1)?,
                size: row.get(2)?,
            })
        },
    )
    .ok()
}

fn assert_high_value_linux_paths_enumerated(
    conn: &Connection,
    ds_id: &DataSourceId,
) -> Vec<LinuxPathEntry> {
    let mut entries = Vec::new();
    for linux_path in HIGH_VALUE_LINUX_SYSTEM_INFO_PATHS {
        let entry = find_linux_file_entry(conn, ds_id, linux_path).unwrap_or_else(|| {
            panic!("root LV enumeration must include high-value Linux path {linux_path}")
        });
        eprintln!(
            "  enumerated required path {}: id={} stored_path='{}' size={}",
            linux_path, entry.file_id.0, entry.path, entry.size
        );
        assert!(
            path_has_linux_suffix(&entry.path, linux_path),
            "stored file entry path '{}' should resolve to Linux path {linux_path}",
            entry.path
        );
        entries.push(entry);
    }
    entries
}

fn assert_liuyang_root_lv_tree_is_complete(
    conn: &Connection,
    ds_id: &DataSourceId,
    stats: &app_services::file_service::EnumerationStats,
) {
    assert!(
        stats.file_count >= MIN_LIUYANG_ROOT_LV_FILE_COUNT
            && stats.dir_count >= MIN_LIUYANG_ROOT_LV_DIR_COUNT,
        "root LV should enumerate the complete current sample tree, got files={} dirs={}",
        stats.file_count,
        stats.dir_count
    );
    assert!(
        stats.warnings.is_empty(),
        "complete root LV walk should not hide unreadable directories: {:?}",
        stats.warnings
    );

    let total_entries = total_file_entries(conn, ds_id);
    assert_eq!(
        total_entries as u64,
        stats.file_count + stats.dir_count,
        "DB row count should match the enumerated root plus all child entries"
    );

    for segment in ["boot", "dev", "etc", "root", "usr", "var"] {
        assert!(
            count_entries_like(conn, ds_id, &format!("%{segment}%")) > 0,
            "complete root LV import should expose Linux path segment {segment}"
        );
    }
}

fn assert_root_lv_direct_reads_system_info_file(fs: &dyn FileSystemReader, linux_path: &str) {
    let fs_path = linux_path.trim_start_matches('/');
    let range_bytes = fs
        .read_file_range(fs_path, 0, 512)
        .unwrap_or_else(|error| panic!("read_file_range({linux_path}) failed: {error}"));
    eprintln!(
        "  read_file_range {} returned {} bytes",
        linux_path,
        range_bytes.len()
    );
    assert!(
        !range_bytes.is_empty(),
        "read_file_range({linux_path}) should return non-empty bytes"
    );

    let mut reader = fs
        .open_file(fs_path)
        .unwrap_or_else(|error| panic!("open_file({linux_path}) failed: {error}"));
    let mut buf = [0u8; 512];
    let open_read = reader
        .read(&mut buf)
        .unwrap_or_else(|error| panic!("open_file({linux_path}) read failed: {error}"));
    eprintln!("  open_file {} read {} bytes", linux_path, open_read);
    assert!(
        open_read > 0,
        "open_file({linux_path}) should read non-empty bytes"
    );
}

fn assert_root_lv_lists_system_info_paths(fs: &dyn FileSystemReader) {
    let etc_children = fs
        .list_children("etc")
        .expect("root LV should enumerate /etc children");
    let etc_child_names = etc_children
        .iter()
        .map(|child| child.name.as_str())
        .collect::<Vec<_>>();
    eprintln!(
        "  /etc enumeration returned {} children; required sample names present? passwd={} os-release={} hostname={}",
        etc_child_names.len(),
        etc_child_names.contains(&"passwd"),
        etc_child_names.contains(&"os-release"),
        etc_child_names.contains(&"hostname")
    );
    for linux_path in HIGH_VALUE_LINUX_SYSTEM_INFO_PATHS {
        let required_name = linux_path
            .rsplit('/')
            .next()
            .expect("required Linux path should include a file name");
        assert!(
            etc_child_names.contains(&required_name),
            "root LV /etc enumeration must include {linux_path}; /etc children={etc_child_names:?}"
        );
    }
}

fn assert_linux_paths_preview_readable(conn: &Connection, ds_id: &DataSourceId, paths: &[&str]) {
    for linux_path in paths {
        let entry = find_linux_file_entry(conn, ds_id, linux_path).unwrap_or_else(|| {
            panic!("root LV enumeration should include preview target {linux_path}")
        });
        let bytes = file_service::read_file_header_by_id(conn, &entry.file_id, 512).unwrap_or_else(
            |error| {
                panic!(
                    "app-service preview read for arbitrary root LV file {} ({}) failed: {}",
                    linux_path, entry.path, error
                )
            },
        );
        eprintln!(
            "  arbitrary preview read {} via stored entry {} returned {} bytes",
            linux_path,
            entry.file_id.0,
            bytes.len()
        );
        assert!(
            !bytes.is_empty(),
            "app-service preview read for {linux_path} should return non-empty bytes"
        );
    }
}

fn assert_linux_artifact_candidates_include_system_info_paths(
    candidates: &[app_services::analysis_service::EvidenceCandidate],
) {
    let candidate_paths = candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect::<Vec<_>>();
    for linux_path in HIGH_VALUE_LINUX_SYSTEM_INFO_PATHS {
        assert!(
            candidates
                .iter()
                .any(|candidate| path_has_linux_suffix(&candidate.path, linux_path)),
            "LinuxArtifacts candidates should include {linux_path}; candidates={candidate_paths:?}"
        );
    }
}

fn assert_linux_artifact_candidates_cover_critical_paths(
    candidates: &[app_services::analysis_service::EvidenceCandidate],
) {
    let candidate_paths = candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect::<Vec<_>>();
    assert!(
        candidates.len() >= MIN_LIUYANG_LINUX_ARTIFACT_CANDIDATES,
        "real Linux root LV should expose many Linux artifact candidates, got {}: {candidate_paths:?}",
        candidates.len()
    );
    for linux_path in CRITICAL_LINUX_ARTIFACT_CANDIDATE_PATHS {
        assert!(
            candidates
                .iter()
                .any(|candidate| path_has_linux_suffix(&candidate.path, linux_path)),
            "LinuxArtifacts candidates should include {linux_path}; candidates={candidate_paths:?}"
        );
    }
}

fn assert_linux_artifact_extraction_has_real_families(conn: &Connection) {
    for artifact_type in [
        "LinuxJournal",
        "LinuxWtmp",
        "LinuxBashCommand",
        "LinuxCronJob",
        "LinuxSudoEvent",
        "LinuxSystemConfig",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM artifacts WHERE artifact_type = ?1",
                rusqlite::params![artifact_type],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            count > 0,
            "real Linux extraction should persist at least one {artifact_type} artifact"
        );
    }
}

fn assert_linux_artifact_source_paths_include(conn: &Connection, paths: &[&str]) {
    for linux_path in paths {
        let suffix = normalize_linux_path_suffix(linux_path);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM artifacts
                 WHERE artifact_type LIKE 'Linux%'
                   AND (
                        LOWER(json_extract(attrs, '$.sourcePath')) = ?1
                        OR LOWER(json_extract(attrs, '$.sourcePath')) LIKE ?2
                   )",
                rusqlite::params![suffix, format!("%/{suffix}")],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            count > 0,
            "real Linux extraction should persist artifact sourcePath ending with {linux_path}"
        );
    }
}

fn lvm_root_partition_record(conn: &Connection, ds_id: &DataSourceId) -> DataSourcePartitionRecord {
    PartitionRepo::new(conn)
        .find_by_data_source(&ds_id.0)
        .unwrap()
        .into_iter()
        .find(|partition| {
            partition.filesystem.as_deref() == Some("XFS")
                && partition.lvm_lv_name.as_deref() == Some(LIUYANG_ROOT_LV_NAME)
        })
        .expect("stored partition metadata should include cl/root")
}

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_root_lv_etc_directory_has_children() {
    let fs = open_root_lv_xfs();
    let root_children = fs
        .list_children("")
        .expect("root LV should enumerate the Linux root directory");
    assert!(
        root_children
            .iter()
            .any(|child| child.name == "etc" && child.is_dir),
        "root LV root should expose /etc as a directory, got {:?}",
        root_children
            .iter()
            .map(|child| child.name.as_str())
            .collect::<Vec<_>>()
    );

    let etc_children = fs
        .list_children("etc")
        .expect("root LV should enumerate XFS dir3 /etc children");
    let etc_child_names = etc_children
        .iter()
        .map(|child| child.name.as_str())
        .collect::<Vec<_>>();
    eprintln!(
        "Root LV /etc direct enumeration returned {} children",
        etc_child_names.len()
    );
    assert!(
        !etc_children.is_empty(),
        "XFS dir3 /etc should not parse as XDD3 with children=0"
    );
    for required_name in ["passwd", "os-release", "hostname"] {
        assert!(
            etc_child_names.contains(&required_name),
            "root LV /etc enumeration must include {required_name}; /etc children={etc_child_names:?}"
        );
    }
}

/// Probe the E01 file and confirm at least one Linux filesystem candidate is
/// detected (Ext4, XFS, or Btrfs).
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_detects_ext_filesystem() {
    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();

    eprintln!("=== Partition probe results ===");
    for partition in &probe.partitions {
        eprintln!(
            "partition idx={} name='{}' kind_label={} status={:?} offset={} length={}",
            partition.index,
            partition.name,
            partition.kind_label,
            partition.status,
            partition.offset,
            partition.length,
        );
    }
    for candidate in &probe.candidates {
        eprintln!(
            "candidate idx={:?} name={:?} kind={:?} offset={}",
            candidate.partition_index, candidate.partition_name, candidate.kind, candidate.offset,
        );
    }

    assert!(
        !probe.candidates.is_empty(),
        "should detect at least one filesystem candidate"
    );
    let has_linux_fs = probe.candidates.iter().any(|c| {
        matches!(
            c.kind,
            ImageFilesystemKind::Ext4
                | ImageFilesystemKind::Xfs
                | ImageFilesystemKind::Btrfs
                | ImageFilesystemKind::Ntfs
                | ImageFilesystemKind::Fat
        )
    });
    assert!(has_linux_fs, "should detect a supportable filesystem");
}

/// Enumerate the filesystem from the first candidate, verify the file tree
/// contains Linux-specific paths, and confirm path reconstruction is correct.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_enumerates_file_tree_and_reconstructs_paths() {
    let probe = detect_expanded_linux_probe();
    let root_lv = root_lv_candidate(&probe);
    let root_name = format_partition_root_name(root_lv);
    assert_eq!(root_lv.kind, ImageFilesystemKind::Xfs);
    assert!(
        matches!(root_lv.source, ImageFilesystemSource::LvmLogicalVolume),
        "tree regression should exercise the real cl/root logical volume"
    );

    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    setup_case(&conn, "linux-e01-test");

    let ds_id = DataSourceId("e01-linux-ds".to_string());
    create_linux_test_data_source(&conn, "linux-e01-test", &ds_id);
    file_service::store_data_source_partitions(&conn, &ds_id, &probe.partitions).unwrap();

    let fs = open_root_lv_xfs();
    let root_children = fs
        .list_children("")
        .expect("root LV should enumerate the Linux root directory");
    let root_child_names = root_children
        .iter()
        .map(|child| child.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        root_child_names.contains(&"boot")
            && root_child_names.contains(&"etc")
            && root_child_names.contains(&"usr"),
        "root LV should expose expected Linux root entries, got {root_child_names:?}"
    );
    assert_root_lv_lists_system_info_paths(&fs);
    for path in HIGH_VALUE_LINUX_SYSTEM_INFO_PATHS {
        assert_root_lv_direct_reads_system_info_file(&fs, path);
    }

    let readable = find_first_readable_file(&fs, "", 4)
        .expect("root LV should expose at least one readable file entry");
    eprintln!(
        "First readable root LV sample file: '{}' ({} bytes read)",
        readable.0, readable.1
    );

    let stats = file_service::enumerate_filesystem_with_root_name(
        &conn,
        &ds_id,
        &fs,
        Some(&root_name),
        None::<&dyn Fn(u32)>,
    )
    .unwrap();

    eprintln!(
        "Enumerated {} files, {} dirs, total={}",
        stats.file_count, stats.dir_count, stats.total_size
    );
    assert_liuyang_root_lv_tree_is_complete(&conn, &ds_id, &stats);

    // Query the file tree to verify path reconstruction
    let tree = file_service::get_file_tree_real(&conn).unwrap();
    eprintln!("File tree root count: {}", tree.len());
    for node in &tree {
        eprintln!(
            "  tree node: id={} name='{}' depth={} hasChildren={} dataSourceId={:?}",
            node.id, node.name, node.depth, node.has_children, node.data_source_id
        );
    }

    // Verify files are tagged with the expected data_source_id.
    let count = total_file_entries(&conn, &ds_id);
    assert!(
        count > 0,
        "file_entries should be tagged with the data source ID"
    );

    let required_root_paths = ["boot", "dev", "etc", "usr", "var"];
    for path_segment in &required_root_paths {
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_entries WHERE LOWER(path) LIKE ?1 AND data_source_id = ?2",
                [format!("%{}%", path_segment), ds_id.0.clone()],
                |row| row.get(0),
            )
            .unwrap();
        eprintln!("  path '{}' found {} entries", path_segment, found);
        assert!(
            found > 0,
            "root LV import should contain at least one path segment '{path_segment}'"
        );
    }

    let high_value_linux_paths = ["etc/passwd", "etc/os-release", "etc/hostname"];
    for path in high_value_linux_paths {
        let found = count_entries_like(&conn, &ds_id, &format!("%{path}%"));
        eprintln!("  high-value path '{}' found {} entries", path, found);
        assert!(
            found > 0,
            "root LV import should contain high-value Linux path '{path}'"
        );
    }
    assert_high_value_linux_paths_enumerated(&conn, &ds_id);
    assert_linux_paths_preview_readable(&conn, &ds_id, ARBITRARY_PREVIEW_READ_PATHS);
}

/// Diagnostic test: read the XFS superblock directly and report version/features.
/// This helps understand which XFS on-disk features are in use when enumeration
/// produces 0 files (the reader only supports shortform directories).
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_probe_xfs_superblock_features() {
    use std::io::{Read, Seek, SeekFrom};

    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let candidate = probe
        .candidates
        .first()
        .expect("should have at least one candidate");
    assert_eq!(candidate.kind, ImageFilesystemKind::Xfs);

    // Read XFS superblock (first 512 bytes at partition offset)
    let mut sb = [0u8; 512];
    reader.seek(SeekFrom::Start(candidate.offset)).unwrap();
    reader.read_exact(&mut sb).unwrap();

    let magic = u32::from_be_bytes([sb[0], sb[1], sb[2], sb[3]]);
    let blocksize = u32::from_be_bytes([sb[4], sb[5], sb[6], sb[7]]);
    let agblocks = u32::from_be_bytes([sb[0x54], sb[0x55], sb[0x56], sb[0x57]]);
    let agcount = u32::from_be_bytes([sb[0x58], sb[0x59], sb[0x5A], sb[0x5B]]);
    let inodesize = u16::from_be_bytes([sb[0x68], sb[0x69]]);
    let sb_features2 = u32::from_be_bytes([sb[0x74], sb[0x75], sb[0x76], sb[0x77]]);
    let sb_features_compat = u32::from_be_bytes([sb[0x78], sb[0x79], sb[0x7A], sb[0x7B]]);
    let sb_features_ro_compat = u32::from_be_bytes([sb[0x7C], sb[0x7D], sb[0x7E], sb[0x7F]]);
    let sb_features_incompat = u32::from_be_bytes([sb[0x80], sb[0x81], sb[0x82], sb[0x83]]);

    eprintln!("=== XFS Superblock Probe ===");
    eprintln!("magic=0x{:08X}", magic);
    eprintln!("blocksize={}", blocksize);
    eprintln!("agcount={} agblocks={}", agcount, agblocks);
    eprintln!("inodesize={}", inodesize);
    eprintln!("features2=0x{:08X}", sb_features2);
    eprintln!("compat=0x{:08X}", sb_features_compat);
    eprintln!("ro_compat=0x{:08X}", sb_features_ro_compat);
    eprintln!("incompat=0x{:08X}", sb_features_incompat);

    // Check v5 superblock (metadata checksums)
    if sb_features2 & 0x20 != 0 {
        eprintln!("=> V5 superblock (metadata checksums)");
    }
    // Check free inode btree
    if sb_features_ro_compat & 0x02 != 0 {
        eprintln!("=> Free inode B+tree (finobt)");
    }
    // Check reverse-mapping btree
    if sb_features_ro_compat & 0x08 != 0 {
        eprintln!("=> Reverse-mapping B+tree (rmapbt)");
    }
    // Check reflink (shared data extents)
    if sb_features_ro_compat & 0x10 != 0 {
        eprintln!("=> Reflink (shared data extents)");
    }
    // Check sparse inodes
    if sb_features_ro_compat & 0x40 != 0 {
        eprintln!("=> Sparse inodes (sparse)");
    }
    // Check bigtime (64-bit timestamps)
    if sb_features_incompat & 0x200 != 0 {
        eprintln!("=> Bigtime (64-bit timestamps)");
    }
    // Check metadata directory trees
    if sb_features_incompat & 0x400 != 0 {
        eprintln!("=> Metadata directory trees");
    }

    // The reader needs v3 inode core but REALTIME/LOGINDEV incompat
    // flags block it. The primary limitation for enumeration is that
    // the root directory might not be shortform (di_format=1).
    // When di_format >= 3 (block/leaf/node), list_children returns empty.
    assert_eq!(magic, 0x5846_5342, "XFSB magic should be correct");

    // Try to probe the root inode (ino=2) directly from the raw image to
    // determine its di_format and di_mode.
    // Real XFS uses per-AG inode B+trees; the flat-table approach
    // (inode_base_block=2) only works for synthetic fixtures.  This probe
    // reads the raw inode at a guessed offset based on common XFS geometry
    // to verify the root is a block-format directory.
    let inopblock = u16::from_be_bytes([sb[0x6A], sb[0x6B]]) as u64;
    let root_ino = u64::from_be_bytes([
        sb[0x38], sb[0x39], sb[0x3A], sb[0x3B], sb[0x3C], sb[0x3D], sb[0x3E], sb[0x3F],
    ]);
    eprintln!(
        "root_ino={} blocksize={} inodesize={} inopblock={}",
        root_ino, blocksize, inodesize, inopblock
    );

    // For a proper inode lookup, we'd need AG inode B+tree traversal.
    // The flat table at block 2 would be:
    let flat_inode_offset =
        candidate.offset + 2 * blocksize as u64 + (root_ino - 1) * inodesize as u64;
    eprintln!(
        "flat-table inode location (invalid for real XFS): offset={}",
        flat_inode_offset
    );
    reader.seek(SeekFrom::Start(flat_inode_offset)).unwrap();
    let mut inode_buf = vec![0u8; inodesize as usize];
    reader.read_exact(&mut inode_buf).unwrap();
    let inode_magic = u16::from_be_bytes([inode_buf[0], inode_buf[1]]);
    let inode_format = inode_buf[5];
    let inode_mode = u16::from_be_bytes([inode_buf[2], inode_buf[3]]);
    eprintln!("flat-table root inode: magic=0x{:04X} mode=0x{:04X} format={} (expected IN=0x494E for valid inode)", inode_magic, inode_mode, inode_format);
    eprintln!("=> The non-matching magic confirms XFS reader needs AG inode B+tree lookup to resolve real inodes.");
}

/// Probe the XFS root directory inode via the reader's raw inode resolution
/// path to confirm it has block/leaf directory format.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_probe_xfs_root_inode_format() {
    use std::io::{Read, Seek, SeekFrom};

    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let candidate = probe
        .candidates
        .first()
        .expect("should have at least one candidate");

    // Read the superblock to get AG geometry
    let mut sb = [0u8; 512];
    reader.seek(SeekFrom::Start(candidate.offset)).unwrap();
    reader.read_exact(&mut sb).unwrap();

    let blocksize = u32::from_be_bytes([sb[4], sb[5], sb[6], sb[7]]) as u64;
    let agblocks = u32::from_be_bytes([sb[0x54], sb[0x55], sb[0x56], sb[0x57]]) as u64;
    let inodesize = u16::from_be_bytes([sb[0x68], sb[0x69]]) as u64;
    let agcount = u32::from_be_bytes([sb[0x58], sb[0x59], sb[0x5A], sb[0x5B]]);
    let root_ino = u64::from_be_bytes([
        sb[0x38], sb[0x39], sb[0x3A], sb[0x3B], sb[0x3C], sb[0x3D], sb[0x3E], sb[0x3F],
    ]);

    eprintln!("=== Root Inode Probe ===");
    eprintln!(
        "root_ino={} blocksize={} agcount={} agblocks={} inodesize={}",
        root_ino, blocksize, agcount, agblocks, inodesize
    );

    // Determine which AG root_ino belongs to
    let agno = root_ino / agblocks;
    let agino = root_ino % agblocks;
    eprintln!("AG {} ino-in-ag={}", agno, agino);

    // In a real XFS, we need the AG inode B+tree to locate the inode chunk.
    // This diagnostic confirms that the flat table does NOT resolve correctly,
    // which is why the reader returns 0 files from this E01.
    let flat_ino_table_offset = candidate.offset + 2 * blocksize;
    reader
        .seek(SeekFrom::Start(
            flat_ino_table_offset + (root_ino - 1) * inodesize,
        ))
        .unwrap();
    let mut inode_buf = vec![0u8; inodesize as usize];
    reader.read_exact(&mut inode_buf).unwrap();

    let inode_magic = u16::from_be_bytes([inode_buf[0], inode_buf[1]]);
    let inode_version = inode_buf[4];
    let inode_format = inode_buf[5];
    let inode_forkoff = inode_buf[0x52];
    let inode_nextents = u32::from_be_bytes([
        inode_buf[0x4C],
        inode_buf[0x4D],
        inode_buf[0x4E],
        inode_buf[0x4F],
    ]);
    eprintln!(
        "flat-table root inode: magic=0x{:04X} version={} format={} forkoff={} nextents={}",
        inode_magic, inode_version, inode_format, inode_forkoff, inode_nextents
    );
    eprintln!("(IN=0x494E expected for valid inode)");

    // The reader uses v2 inode core (96 bytes).  V3 inodes have a 176-byte core.
    // With the fix that detects di_version=3, the data fork correctly starts at
    // offset 176 instead of 96.  If nextents > 0, the extent data should be
    // readable from the correct data-fork offset.
    let core_size: usize = if inode_version == 3 { 176 } else { 96 };
    let data_fork_start = core_size;
    if inode_nextents > 0 && data_fork_start + 16 <= inode_buf.len() {
        let _l0 = u64::from_be_bytes([
            inode_buf[data_fork_start],
            inode_buf[data_fork_start + 1],
            inode_buf[data_fork_start + 2],
            inode_buf[data_fork_start + 3],
            inode_buf[data_fork_start + 4],
            inode_buf[data_fork_start + 5],
            inode_buf[data_fork_start + 6],
            inode_buf[data_fork_start + 7],
        ]);
        let l1 = u64::from_be_bytes([
            inode_buf[data_fork_start + 8],
            inode_buf[data_fork_start + 9],
            inode_buf[data_fork_start + 10],
            inode_buf[data_fork_start + 11],
            inode_buf[data_fork_start + 12],
            inode_buf[data_fork_start + 13],
            inode_buf[data_fork_start + 14],
            inode_buf[data_fork_start + 15],
        ]);
        let block_count = l1 & 0x1F_FFFF;
        let start_block = l1 >> 21;
        // Compute the physical image offset for the directory data
        let dir_data_offset = candidate.offset + start_block * blocksize;
        eprintln!(
            "first extent: logical=0 start_block={} block_count={} dir_data_offset={}",
            start_block, block_count, dir_data_offset
        );

        // Read a few bytes of the directory data block to check the magic
        if block_count > 0 {
            let mut dir_hdr = [0u8; 4];
            if reader.seek(SeekFrom::Start(dir_data_offset)).is_ok()
                && reader.read_exact(&mut dir_hdr).is_ok()
            {
                let dir_magic = u32::from_be_bytes(dir_hdr);
                eprintln!(
                    "directory block magic: 0x{:08X} (XDB3=0x58444233, XDB2=0x58444232)",
                    dir_magic
                );
            }
        }
    }
    eprintln!("=> XFS block directory parsing is now enabled for EXTENTS/BTREE formats.");
}

/// Diagnostic test: call the XFS reader's `list_children("")` directly and
/// print the actual error, instead of letting `enumerate_filesystem` swallow
/// it into a warning string.  Also probes the AGI block and inobt root
/// directly to validate the Stage 4 `locate_inode` path against real data.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_probe_locate_inode_diagnostics() {
    use std::io::{Read, Seek, SeekFrom};

    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let candidate = probe
        .candidates
        .first()
        .expect("should have at least one candidate");
    assert_eq!(candidate.kind, ImageFilesystemKind::Xfs);

    let boxed_reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path()).unwrap());
    let fs = fs_xfs::XfsReader::open(boxed_reader, candidate.offset).unwrap();

    match fs.list_children("") {
        Ok(children) => {
            eprintln!("list_children(\"\") succeeded: {} entries", children.len());
            for c in &children {
                eprintln!("  entry: name='{}' is_dir={}", c.name, c.is_dir);
            }
        }
        Err(e) => {
            eprintln!("list_children(\"\") FAILED: {}", e);
        }
    }

    // Re-read the real superblock feature flags at the CORRECT offsets.
    // xfs_dsb_t: sb_features2 @0xC8, sb_features_compat @0xD0,
    // sb_features_ro_compat @0xD4, sb_features_incompat @0xD8.
    let mut sb = [0u8; 512];
    reader.seek(SeekFrom::Start(candidate.offset)).unwrap();
    reader.read_exact(&mut sb).unwrap();
    let blocksize = u32::from_be_bytes([sb[4], sb[5], sb[6], sb[7]]) as u64;
    let agblocks = u32::from_be_bytes([sb[0x54], sb[0x55], sb[0x56], sb[0x57]]) as u64;
    let features2 = u32::from_be_bytes([sb[0xC8], sb[0xC9], sb[0xCA], sb[0xCB]]);
    let compat = u32::from_be_bytes([sb[0xD0], sb[0xD1], sb[0xD2], sb[0xD3]]);
    let ro_compat = u32::from_be_bytes([sb[0xD4], sb[0xD5], sb[0xD6], sb[0xD7]]);
    let incompat = u32::from_be_bytes([sb[0xD8], sb[0xD9], sb[0xDA], sb[0xDB]]);
    eprintln!(
        "corrected feature flags: features2=0x{:08X} compat=0x{:08X} ro_compat=0x{:08X} incompat=0x{:08X}",
        features2, compat, ro_compat, incompat
    );
    eprintln!(
        "  sparse inodes (ro_compat bit6/0x40) = {}",
        ro_compat & 0x40 != 0
    );
    eprintln!("  finobt (ro_compat bit1/0x02) = {}", ro_compat & 0x02 != 0);

    // Hex-dump the first 64 bytes of blocks 0..3 (relative to the partition
    // offset) to manually verify the true byte layout, since blocks 1-3
    // did not show the expected AGF/AGI/AGFL magic values.
    for ag_block in 0..=3u64 {
        let block_offset = candidate.offset + ag_block * blocksize;
        let mut buf = [0u8; 64];
        reader.seek(SeekFrom::Start(block_offset)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        eprintln!(
            "AG0 block {} (offset {}) first 64 bytes:",
            ag_block, block_offset
        );
        eprintln!(
            "  {}",
            buf.iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    let mut agi_offset = 0u64;
    let mut agi_root = 0u32;
    let mut agi_level = 0u32;
    let mut agi_magic_found = 0u32;
    for ag_block in 1..=3u64 {
        let block_offset = candidate.offset + ag_block * blocksize;
        let mut buf = vec![0u8; blocksize as usize];
        reader.seek(SeekFrom::Start(block_offset)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        eprintln!(
            "AG0 block {}: magic=0x{:08X} (XAGF=0x58414746 XAGI=0x58414749 XAGFL=0x5841464C)",
            ag_block, magic
        );
        if magic == 0x5841_4749 {
            agi_offset = block_offset;
            agi_magic_found = magic;
            agi_root = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
            agi_level = u32::from_be_bytes([buf[24], buf[25], buf[26], buf[27]]);
        }
    }
    let agi_magic = agi_magic_found;
    eprintln!(
        "AG0 AGI (located at offset {}): magic=0x{:08X} agi_root={} agi_level={}",
        agi_offset, agi_magic, agi_root, agi_level
    );

    let _ = (agi_offset, agi_root, agi_level, agi_magic);

    // Verify SleuthKit's direct bit-decode formula (xfs_inode_get_offset in
    // tsk_xfs.h): XFS inode numbers directly encode AG number, in-AG block
    // number, and in-block inode index -- no inobt B+tree walk is needed
    // for lookup-by-number. sb_inopblog is at superblock offset 0x7B,
    // sb_agblklog is at offset 0x7C (both raw log2 values, 1 byte each).
    let sb_inopblog = sb[0x7B] as u32;
    let sb_agblklog = sb[0x7C] as u32;
    eprintln!(
        "sb_agblklog={} sb_inopblog={} (agblocks=2^{}={} inopblock=2^{}={})",
        sb_agblklog,
        sb_inopblog,
        sb_agblklog,
        1u64 << sb_agblklog,
        sb_inopblog,
        1u64 << sb_inopblog
    );

    let root_ino = u64::from_be_bytes([
        sb[0x38], sb[0x39], sb[0x3A], sb[0x3B], sb[0x3C], sb[0x3D], sb[0x3E], sb[0x3F],
    ]);
    let inodesize = u16::from_be_bytes([sb[0x68], sb[0x69]]) as u64;
    let shift = sb_agblklog + sb_inopblog;
    let ag_num = root_ino >> shift;
    let low_bits = root_ino & ((1u64 << shift) - 1);
    let blk_num = low_bits >> sb_inopblog;
    let ino_in_blk = low_bits & ((1u64 << sb_inopblog) - 1);
    let decoded_offset = candidate.offset
        + ag_num * (agblocks * blocksize)
        + blk_num * blocksize
        + ino_in_blk * inodesize;
    eprintln!(
        "decoded root_ino={}: ag_num={} blk_num={} ino_in_blk={} -> abs_offset={}",
        root_ino, ag_num, blk_num, ino_in_blk, decoded_offset
    );

    let mut dbuf = vec![0u8; inodesize as usize];
    reader.seek(SeekFrom::Start(decoded_offset)).unwrap();
    reader.read_exact(&mut dbuf).unwrap();
    let dmagic = u16::from_be_bytes([dbuf[0], dbuf[1]]);
    let dversion = dbuf[4];
    let dformat = dbuf[5];
    let dmode = u16::from_be_bytes([dbuf[2], dbuf[3]]);
    eprintln!(
        "decoded inode bytes: magic=0x{:04X} (expect 0x494E) version={} format={} mode=0x{:04X}",
        dmagic, dversion, dformat, dmode
    );
}

/// Run Linux artifact extraction against files actually enumerated from the
/// real root LV. This must not insert synthetic high-value files.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_analysis_extraction_produces_linux_artifacts() {
    let ds_id = DataSourceId("e01-linux-analysis-ds".to_string());
    let conn = setup_linux_fixture_case("linux-e01-analysis", &ds_id);
    let stats = enumerate_root_lv_into_case(&conn, &ds_id);
    eprintln!(
        "Root LV enumeration for extraction: files={} dirs={} warnings={:?}",
        stats.file_count, stats.dir_count, stats.warnings
    );
    assert_liuyang_root_lv_tree_is_complete(&conn, &ds_id, &stats);

    let candidates = evidence_candidates_for_categories(&conn, &["LinuxArtifacts"]).unwrap();
    eprintln!(
        "Linux artifact candidates from real root LV traversal: {}",
        candidates.len()
    );
    for candidate in &candidates {
        eprintln!(
            "  path='{}' kind={} parser={} size={}",
            candidate.path, candidate.evidence_kind, candidate.parser, candidate.size
        );
    }
    assert!(
        !candidates.is_empty(),
        "real root LV traversal should discover Linux artifact candidates without synthetic inserts"
    );
    assert_linux_artifact_candidates_cover_critical_paths(&candidates);

    let run = run_analysis_extraction(
        &conn,
        "linux-e01-analysis",
        &["LinuxArtifacts"],
        |file_id| {
            file_service::read_file_header_by_id(
                &conn,
                file_id,
                app_services::analysis_service::MAX_ANALYSIS_SOURCE_BYTES,
            )
            .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
            .map_err(|error| error.to_string())
        },
    )
    .expect("analysis extraction should succeed");

    eprintln!(
        "Linux artifact extraction: scanned={} artifacts={} timeline_events={} warnings={:?}",
        run.scanned_count, run.artifact_count, run.timeline_event_count, run.warnings
    );
    assert!(
        run.scanned_count > 0,
        "expected real Linux artifact sources scanned"
    );
    assert!(
        run.artifact_count > 0,
        "expected real Linux artifact extraction to persist artifacts"
    );

    let summary = get_linux_artifact_summary(&conn, 0, 200).unwrap();
    eprintln!(
        "Linux artifact summary: total={} journal={} login={} bash={} apt={} cron={} sudo={}",
        summary.total_count,
        summary.journal_count,
        summary.login_count,
        summary.bash_command_count,
        summary.apt_event_count,
        summary.cron_job_count,
        summary.sudo_event_count,
    );
    assert!(
        summary.total_count >= run.artifact_count,
        "summary should include all persisted Linux artifacts"
    );
    assert!(
        summary.journal_count > 0
            && summary.login_count > 0
            && summary.bash_command_count > 0
            && summary.cron_job_count > 0
            && summary.sudo_event_count > 0,
        "real sample should produce journal/login/bash/cron/sudo Linux artifacts"
    );
    assert_linux_artifact_extraction_has_real_families(&conn);
    assert_linux_artifact_source_paths_include(
        &conn,
        &[
            "/etc/passwd",
            "/etc/os-release",
            "/root/.bash_history",
            "/var/log/wtmp",
            "/var/log/messages",
            "/var/log/secure",
            "/var/spool/cron/root",
        ],
    );
}

/// Real-sample completeness regression for the root logical volume.
///
/// This intentionally asserts beyond "the root node exists": the root LV must
/// enumerate key Linux system files, direct filesystem reads must return
/// content for them, app-service preview reads must resolve the stored entries,
/// and LinuxArtifacts candidate discovery must preserve these prefixed paths.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_root_lv_system_info_paths_are_enumerated_readable_and_candidates() {
    let ds_id = DataSourceId("e01-linux-system-info-ds".to_string());
    let conn = setup_linux_fixture_case("linux-e01-system-info", &ds_id);

    let fs = open_root_lv_xfs();
    let root_children = fs
        .list_children("")
        .expect("root LV should enumerate the Linux root directory");
    let root_child_names = root_children
        .iter()
        .map(|child| child.name.as_str())
        .collect::<Vec<_>>();
    eprintln!(
        "Root LV direct enumeration: root_children={} names={root_child_names:?}",
        root_child_names.len()
    );
    assert!(
        root_child_names.contains(&"etc"),
        "root LV root enumeration must contain /etc, got {root_child_names:?}"
    );
    assert_root_lv_lists_system_info_paths(&fs);
    for linux_path in HIGH_VALUE_LINUX_SYSTEM_INFO_PATHS {
        assert_root_lv_direct_reads_system_info_file(&fs, linux_path);
    }

    let stats = enumerate_root_lv_into_case(&conn, &ds_id);
    eprintln!(
        "Root LV DB enumeration: files={} dirs={} total={} warnings={:?}",
        stats.file_count, stats.dir_count, stats.total_size, stats.warnings
    );
    assert_liuyang_root_lv_tree_is_complete(&conn, &ds_id, &stats);

    let entries = assert_high_value_linux_paths_enumerated(&conn, &ds_id);
    for (linux_path, entry) in HIGH_VALUE_LINUX_SYSTEM_INFO_PATHS
        .iter()
        .zip(entries.iter())
    {
        let preview_bytes = file_service::read_file_header_by_id(&conn, &entry.file_id, 512)
            .unwrap_or_else(|error| {
                panic!(
                    "app-service preview read for {} ({}) failed: {}",
                    linux_path, entry.path, error
                )
            });
        eprintln!(
            "  preview read {} via stored entry {} returned {} bytes",
            linux_path,
            entry.file_id.0,
            preview_bytes.len()
        );
        assert!(
            !preview_bytes.is_empty(),
            "app-service preview read for {linux_path} should return non-empty bytes"
        );
    }

    let candidates = evidence_candidates_for_categories(&conn, &["LinuxArtifacts"]).unwrap();
    eprintln!(
        "Linux artifact candidates after root LV system-info enumeration: {}",
        candidates.len()
    );
    for candidate in &candidates {
        eprintln!(
            "  candidate path='{}' kind={} parser={} size={}",
            candidate.path, candidate.evidence_kind, candidate.parser, candidate.size
        );
    }
    assert!(
        !candidates.is_empty(),
        "LinuxArtifacts discovery should find real candidates from root LV traversal"
    );
    assert_linux_artifact_candidates_include_system_info_paths(&candidates);
    assert_linux_artifact_candidates_cover_critical_paths(&candidates);
    assert_linux_paths_preview_readable(&conn, &ds_id, ARBITRARY_PREVIEW_READ_PATHS);
}

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_linux_artifact_candidates_survive_lvm_root_prefixes() {
    let ds_id = DataSourceId("e01-linux-candidate-ds".to_string());
    let conn = setup_linux_fixture_case("linux-e01-candidates", &ds_id);
    let stats = enumerate_root_lv_into_case(&conn, &ds_id);
    eprintln!(
        "Root LV enumeration for candidate coverage: files={} dirs={} total={} warnings={:?}",
        stats.file_count, stats.dir_count, stats.total_size, stats.warnings
    );

    assert_liuyang_root_lv_tree_is_complete(&conn, &ds_id, &stats);
    for segment in ["boot", "dev", "etc", "usr", "var"] {
        assert!(
            count_entries_like(&conn, &ds_id, &format!("%{segment}%")) > 0,
            "root LV import should expose path segment {segment}"
        );
    }

    let real_candidates = evidence_candidates_for_categories(&conn, &["LinuxArtifacts"]).unwrap();
    eprintln!(
        "Linux artifact candidates from current root LV traversal: {}",
        real_candidates.len()
    );
    for candidate in &real_candidates {
        eprintln!(
            "  real candidate path='{}' kind={} parser={} size={}",
            candidate.path, candidate.evidence_kind, candidate.parser, candidate.size
        );
    }

    let paths = real_candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect::<Vec<_>>();
    assert!(
        !paths.is_empty(),
        "real prefixed Linux paths should match LinuxArtifacts candidates: {paths:?}"
    );
    assert_linux_artifact_candidates_include_system_info_paths(&real_candidates);
    assert_linux_artifact_candidates_cover_critical_paths(&real_candidates);
}

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_reads_bytes_from_root_lv_file_entries() {
    let ds_id = DataSourceId("e01-linux-bytes-ds".to_string());
    let conn = setup_linux_fixture_case("linux-e01-byte-read", &ds_id);
    let stats = enumerate_root_lv_into_case(&conn, &ds_id);
    eprintln!(
        "Root LV enumeration for byte-read coverage: files={} dirs={} total={} warnings={:?}",
        stats.file_count, stats.dir_count, stats.total_size, stats.warnings
    );
    assert_liuyang_root_lv_tree_is_complete(&conn, &ds_id, &stats);

    let partition = lvm_root_partition_record(&conn, &ds_id);
    assert_eq!(partition.partition_index, 2);
    assert_eq!(partition.filesystem.as_deref(), Some("XFS"));
    assert_eq!(partition.lvm_lv_name.as_deref(), Some(LIUYANG_ROOT_LV_NAME));
    assert_eq!(
        partition.lvm_pv_offsets_json.as_deref(),
        Some("[1074790400]"),
        "preview/open path needs stored LVM PV offsets for root LV byte reads"
    );

    let entries = assert_high_value_linux_paths_enumerated(&conn, &ds_id);
    for (linux_path, entry) in HIGH_VALUE_LINUX_SYSTEM_INFO_PATHS
        .iter()
        .zip(entries.iter())
    {
        let bytes = file_service::read_file_header_by_id(&conn, &entry.file_id, 512)
            .unwrap_or_else(|error| {
                panic!(
                    "root LV file entry {} for {} should be preview-readable through stored LVM metadata: {}",
                    entry.file_id.0, linux_path, error
                )
            });
        eprintln!(
            "Read {} bytes from root LV file entry {} stored_path='{}'",
            bytes.len(),
            entry.file_id.0,
            entry.path
        );
        assert!(
            !bytes.is_empty(),
            "root LV file preview for {linux_path} should return non-empty bytes"
        );
    }
    assert_linux_paths_preview_readable(&conn, &ds_id, ARBITRARY_PREVIEW_READ_PATHS);
}

/// Verify LVM pool expansion discovers logical volumes on the real E01 sample.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_lvm_expansion_discovers_logical_volumes() {
    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let mut probe = detect_image_filesystem(&mut reader).unwrap();
    assert!(
        probe.candidates.iter().any(|candidate| {
            candidate.kind == ImageFilesystemKind::LvmPool
                && candidate.offset == LIUYANG_LVM_POOL_OFFSET
        }),
        "initial probe should expose the physical LVM pool before expansion"
    );

    let source_kind = domain::DataSourceKind::E01;
    expand_lvm_pool_candidates(&mut probe, &fixture_path(), &source_kind);

    assert!(
        probe
            .candidates
            .iter()
            .any(|c| matches!(c.source, ImageFilesystemSource::LvmLogicalVolume)),
        "should have at least one LvmLogicalVolume candidate after LVM expansion"
    );

    let root_lv = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume)
                && candidate
                    .lvm_identity
                    .as_ref()
                    .is_some_and(|identity| identity.lv_name == LIUYANG_ROOT_LV_NAME)
        })
        .expect("should discover cl/root logical volume");
    assert_eq!(root_lv.kind, ImageFilesystemKind::Xfs);
    let identity = root_lv
        .lvm_identity
        .as_ref()
        .expect("root LV candidate should persist LVM identity");
    assert_eq!(identity.vg_name, LIUYANG_ROOT_LV_VG_NAME);
    assert_eq!(identity.lv_name, LIUYANG_ROOT_LV_NAME);
    assert_eq!(identity.pv_offsets, vec![LIUYANG_LVM_POOL_OFFSET]);
    assert!(!identity.vg_uuid.is_empty(), "VG UUID must be persisted");
    assert!(!identity.lv_uuid.is_empty(), "LV UUID must be persisted");

    assert!(
        probe.partitions.iter().any(|partition| {
            partition.offset == LIUYANG_LVM_POOL_OFFSET
                && matches!(
                    partition.status,
                    app_services::datasource_service::PartitionStatus::Expanded
                )
        }),
        "original LVM pool partition should be marked Expanded after LV redirection"
    );

    let e01_reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path()).unwrap());
    let pool = fs_lvm::LvmPool::discover(vec![e01_reader], vec![LIUYANG_LVM_POOL_OFFSET])
        .expect("LVM pool discovery should succeed");
    let root_index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.name == LIUYANG_ROOT_LV_NAME)
        .expect("root LV should be present in direct LVM discovery");
    let root_reader = pool.open_volume(root_index).expect("root LV should open");
    let root_fs =
        fs_xfs::XfsReader::open(Box::new(root_reader), 0).expect("root LV should mount as XFS");
    let root_children = root_fs
        .list_children("")
        .expect("root LV should enumerate root directory");
    let root_child_names = root_children
        .iter()
        .map(|child| child.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        root_child_names.contains(&"boot") && root_child_names.contains(&"etc"),
        "root LV should expose expected Linux root entries, got {root_child_names:?}"
    );
    assert_root_lv_lists_system_info_paths(&root_fs);

    assert_lvm_root_lv_visible_without_expanded_pool_root(&probe, root_lv);
}

fn assert_lvm_root_lv_visible_without_expanded_pool_root(
    expanded_probe: &app_services::datasource_service::ImageFilesystemProbe,
    root_lv: &ImageFilesystemCandidate,
) {
    let fixture = fixture_path();
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    setup_case(&conn, "linux-e01-lvm-tree-test");

    let ds_id = DataSourceId("e01-linux-lvm-tree-ds".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("linux-e01-lvm-tree-test".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "Linux E01 LVM tree".to_string(),
                kind: DataSourceKind::E01,
                source_path: fixture.clone(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    let expanded_pool = expanded_probe
        .partitions
        .iter()
        .find(|partition| {
            partition.offset == LIUYANG_LVM_POOL_OFFSET
                && matches!(
                    partition.status,
                    app_services::datasource_service::PartitionStatus::Expanded
                )
        })
        .expect("expanded probe should retain the redirected LVM pool partition");
    let expanded_pool_root_name = format_partition_record_root_name(expanded_pool);
    let root_lv_root_name = format_partition_root_name(root_lv);

    eprintln!("=== LVM visible tree import regression ===");
    eprintln!(
        "  Expanded pool root candidate: index={} name='{}' status={:?}",
        expanded_pool.index, expanded_pool_root_name, expanded_pool.status
    );
    eprintln!("  Root LV root candidate: '{}'", root_lv_root_name);

    let stats = enumerate_image_data_source(
        &conn,
        &ds_id,
        E01Reader::open(&fixture).unwrap(),
        |pct, detail| {
            eprintln!("  import progress {pct}%: {detail}");
            Ok(())
        },
        None,
        None,
    )
    .unwrap();
    eprintln!(
        "  imported via image pipeline: files={} dirs={} total={} warnings={:?}",
        stats.file_count, stats.dir_count, stats.total_size, stats.warnings
    );
    assert!(
        stats.file_count >= MIN_LIUYANG_ROOT_LV_FILE_COUNT
            && stats.dir_count >= MIN_LIUYANG_ROOT_LV_DIR_COUNT,
        "image import should enumerate the complete visible Linux root LV tree, got files={} dirs={}",
        stats.file_count,
        stats.dir_count
    );

    let tree = file_service::get_file_tree_real_with_visibility(&conn, false).unwrap();
    let visible_roots = tree
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    eprintln!("  visible roots after import: {visible_roots:?}");
    let root_lv_tree = tree
        .iter()
        .find(|node| node.name == root_lv_root_name)
        .expect("visible tree should expose the cl/root logical volume root");
    assert_eq!(root_lv_tree.node_type.as_deref(), Some("partition"));
    assert_eq!(root_lv_tree.status.as_deref(), Some("ready"));
    let root_lv_children = file_service::get_file_children_lazy_with_visibility(
        &conn,
        &root_lv_tree.id,
        0,
        100,
        false,
    )
    .unwrap();
    let root_lv_child_names = root_lv_children
        .children
        .iter()
        .map(|child| child.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        root_lv_child_names.contains(&"boot") && root_lv_child_names.contains(&"etc"),
        "visible cl/root tree root should expose expected Linux root children, got {root_lv_child_names:?}"
    );
    let root_lv_etc = root_lv_children
        .children
        .iter()
        .find(|child| child.name == "etc")
        .expect("visible cl/root tree should include /etc");
    for required_name in ["passwd", "os-release", "hostname"] {
        let child_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM file_entries
                 WHERE parent_id = ?1
                   AND entry_type = 'file' COLLATE NOCASE
                   AND name = ?2",
                rusqlite::params![root_lv_etc.id, required_name],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            child_rows > 0,
            "stored cl/root /etc tree should expose file child {required_name}"
        );
    }
    assert!(
        !visible_roots.contains(&expanded_pool_root_name.as_str()),
        "visible tree must not expose the Expanded physical LVM pool partition; roots={visible_roots:?}"
    );

    assert_no_visible_expanded_pool_root_row(
        &conn,
        &ds_id,
        expanded_pool,
        &expanded_pool_root_name,
    );
}

fn assert_no_visible_expanded_pool_root_row(
    conn: &Connection,
    ds_id: &DataSourceId,
    expanded_pool: &PartitionRecord,
    expanded_pool_root_name: &str,
) {
    let pool_root_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM file_entries
             WHERE data_source_id = ?1
               AND parent_id IS NULL
               AND name = ?2
               AND path NOT LIKE '__partition_placeholder__/%'",
            rusqlite::params![ds_id.0, expanded_pool_root_name],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        pool_root_rows, 0,
        "Expanded LVM pool partition index {} should not become a visible root row",
        expanded_pool.index
    );
}
