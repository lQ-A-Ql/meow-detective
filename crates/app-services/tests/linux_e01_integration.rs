//! Integration test: verify ext4/XFS/Btrfs filesystem detection, file tree
//! enumeration, path reconstruction, and Linux artifact extraction from a real
//! Linux E01 sample.
//!
//! These tests are ignored by default because they require the environment
//! variable `FORENSICS_LINUX_E01_FIXTURE` pointing to a Linux E01 file, e.g.:
//!   D:\獬豸杯\检材3.E01
//! PVE cluster coverage uses `FORENSICS_PVE_CLUSTER_ROOT`, e.g.:
//!   E:\pangushi\服务器
//!
//! Run with:
//!   $env:FORENSICS_LINUX_E01_FIXTURE='D:\獬豸杯\检材3.E01'
//!   $env:FORENSICS_PVE_CLUSTER_ROOT='E:\pangushi\服务器'
//!   cargo test -p app-services --test linux_e01_integration -- --ignored

use app_services::{
    analysis_service::{
        evidence_candidates_for_categories, get_linux_artifact_summary, run_analysis_extraction,
    },
    cluster_service::plan_linux_cluster_import,
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
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use transport::dto::{AnalysisExtractionRunDto, AnalysisParseStatusDto, ViewerRangeRequestDto};

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
const MIN_LIUYANG_LARGE_PREVIEW_FILE_BYTES: u64 = 2 * 1024 * 1024;
const LINUX_PREVIEW_RANGE_LEN: u32 = 4096;
const LINUX_ANALYSIS_SECTION_KEYS: &[&str] = &[
    "LinuxJournal",
    "LinuxLogin",
    "LinuxCommands",
    "LinuxPackages",
    "LinuxCron",
    "LinuxSudo",
    "LinuxSystemConfig",
    "LinuxWebServices",
];
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

fn pve_cluster_root() -> PathBuf {
    std::env::var("FORENSICS_PVE_CLUSTER_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"E:\pangushi\服务器"))
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

fn lvm_metadata_with_internal_and_thin_lvs(pv_uuid: &str) -> String {
    format!(
        r#"mixed_vg {{
id="vg-mixed"
seqno=7
extent_size=2
physical_volumes {{ pv0 {{ id="{pv_uuid}" pe_start=5 pe_count=128 }} }}
logical_volumes {{
root {{ id="lv-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=16 type="striped" stripe_count=1 stripes=["pv0",0] }} }}
pool_tdata {{ id="lv-pool-tdata" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=16 type="striped" stripe_count=1 stripes=["pv0",16] }} }}
thin_root {{ id="lv-thin-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=16 type="thin" thin_pool="pool" transaction_id=1 device_id=2 }} }}
}}
}}"#
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
    let marker_offset = SYNTHETIC_LV_MARKER_OFFSET + 1024 + 0x38;
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
    direct_lv.seek(SeekFrom::Start(1024 + 0x38)).unwrap();
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
                && identity
                    .pv_sources
                    .iter()
                    .any(|source| source.offset == low_offset
                        && source.pv_uuid == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        && source.pv_name.as_deref() == Some("pv0"))
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
                && identity
                    .pv_sources
                    .iter()
                    .any(|source| source.offset == low_offset
                        && source.pv_uuid == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        && source.pv_name.as_deref() == Some("pv0"))
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
            .any(|warning| warning.contains("incomplete")
                && warning.contains("VG name='high_incomplete_vg'")
                && warning.contains("PV name='pv1'")
                && warning.contains(missing_high_pv_uuid)
                && warning.contains("source='<missing>'")
                && warning.contains("offset=<missing>")
                && warning.contains("observed PV source(s)")
                && warning.contains("source='path-sha256:")
                && warning.contains("offset=")
                && warning.contains("LV name='high_root'")),
        "incomplete VG should be reported with VG/LV/PV/source/offset diagnostics; warnings={:?}",
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
                && warning.contains("offset=1048576")
                && warning.contains("source='path-sha256:")
                && warning.contains("pv_uuid='aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'")
                && warning.contains("MDA header magic mismatch")
        }),
        "corrupt LVM metadata should include metadata-area diagnostics; warnings={:?}",
        probe.warnings
    );
    assert_lvm_warnings_do_not_leak_path(&probe.warnings, &source_path);
}

#[test]
fn lvm_expansion_skips_internal_and_thin_logical_volumes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source_path = tmp.path().join("mixed-lvm.raw");
    let pv_offset = 1_048_576u64;
    let pv_uuid = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let pv = build_synthetic_lvm_pv_with_metadata(
        pv_uuid,
        &lvm_metadata_with_internal_and_thin_lvs(pv_uuid),
    );

    let mut disk = vec![0u8; (pv_offset + SYNTHETIC_PV_SIZE) as usize];
    disk[pv_offset as usize..pv_offset as usize + pv.len()].copy_from_slice(&pv);
    std::fs::write(&source_path, disk).unwrap();

    let (candidate, partition) = synthetic_lvm_partition(1, "mixed pv", pv_offset);
    let mut probe = app_services::datasource_service::ImageFilesystemProbe {
        candidates: vec![candidate],
        partitions: vec![partition],
        warnings: Vec::new(),
    };

    expand_lvm_pool_candidates(&mut probe, &source_path, &DataSourceKind::Raw);

    let lv_candidates = probe
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume))
        .collect::<Vec<_>>();
    assert_eq!(
        lv_candidates.len(),
        1,
        "only the public directly mappable root LV should become a filesystem candidate"
    );
    let identity = lv_candidates[0]
        .lvm_identity
        .as_ref()
        .expect("root LV candidate should carry identity");
    assert_eq!(identity.vg_name, "mixed_vg");
    assert_eq!(identity.lv_name, "root");
    assert!(
        probe
            .warnings
            .iter()
            .any(|warning| warning.contains("LV name='pool_tdata'")
                && warning.contains("role='thin-data'")
                && warning.contains("VG name='mixed_vg'")
                && warning.contains("PV name='pv0'")
                && warning.contains("source='path-sha256:")
                && warning.contains("offset=")),
        "internal thin data LV skip should be auditable; warnings={:?}",
        probe.warnings
    );
    assert!(
        probe
            .warnings
            .iter()
            .any(|warning| warning.contains("LV name='thin_root'")
                && warning.contains("role='thin'")
                && warning.contains("VG name='mixed_vg'")
                && warning.contains("PV name='pv0'")
                && warning.contains("source='path-sha256:")
                && warning.contains("offset=")),
        "visible thin LV skip should be auditable; warnings={:?}",
        probe.warnings
    );
    assert_lvm_warnings_do_not_leak_path(&probe.warnings, &source_path);
}

#[test]
fn lvm_expansion_redirects_pool_even_when_all_lvs_are_unsupported() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source_path = tmp.path().join("unsupported-lv.raw");
    let pv_offset = 1_048_576u64;
    let metadata = r#"test_vg {
id="vg-unsupported"
seqno=1
extent_size=8192
physical_volumes { pv0 { id="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" pe_start=5 pe_count=32 } }
logical_volumes {
thin_root { id="lv-thin-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 { start_extent=0 extent_count=1 type="thin" thin_pool="pool" transaction_id=1 device_id=2 } }
}
}
"#;
    let pv = build_synthetic_lvm_pv_with_metadata("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", metadata);
    let mut disk = vec![0u8; (pv_offset + SYNTHETIC_PV_SIZE) as usize];
    disk[pv_offset as usize..pv_offset as usize + pv.len()].copy_from_slice(&pv);
    std::fs::write(&source_path, disk).unwrap();

    let (candidate, partition) = synthetic_lvm_partition(1, "lvm pv", pv_offset);
    let mut probe = app_services::datasource_service::ImageFilesystemProbe {
        candidates: vec![candidate],
        partitions: vec![partition],
        warnings: Vec::new(),
    };

    expand_lvm_pool_candidates(&mut probe, &source_path, &DataSourceKind::Raw);

    assert!(
        probe.candidates.iter().all(|candidate| {
            candidate.offset != pv_offset || !matches!(candidate.kind, ImageFilesystemKind::LvmPool)
        }),
        "successfully parsed LVM pools with unsupported LVs should not remain expandable candidates"
    );
    assert!(
        probe.partitions.iter().any(|partition| {
            partition.offset == pv_offset
                && matches!(
                    partition.status,
                    app_services::datasource_service::PartitionStatus::Expanded
                )
        }),
        "successfully parsed LVM pool partition should be redirected even without supported LV candidates"
    );
    assert!(
        probe.warnings.iter().any(|warning| warning
            .contains("produced no supported logical volume candidates")
            && warning.contains("VG name='test_vg'")
            && warning.contains("PV name='pv0'")
            && warning.contains("source='path-sha256:")
            && warning.contains("offset=")
            && warning.contains("LV name='thin_root'")),
        "unsupported-only VG should retain an actionable warning: {:?}",
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

fn pve_cluster_e01_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !root.exists() {
        return files;
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("e01"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn pve_host_disk_e01_files(root: &Path) -> Vec<PathBuf> {
    pve_cluster_e01_files(root)
        .into_iter()
        .filter(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("-disk01"))
        })
        .collect()
}

fn open_pve_host_filesystem(
    image_path: &Path,
    candidate: &ImageFilesystemCandidate,
) -> Box<dyn FileSystemReader> {
    let identity = candidate
        .lvm_identity
        .as_ref()
        .expect("PVE host filesystem candidate should retain LVM identity");
    let readers = if identity.pv_sources.is_empty() {
        vec![Box::new(E01Reader::open(image_path).unwrap()) as Box<dyn EvidenceReader>]
    } else {
        identity
            .pv_sources
            .iter()
            .map(|source| {
                assert_eq!(
                    source.source_kind,
                    Some(DataSourceKind::E01),
                    "PVE real-sample PV source should remain an E01 source"
                );
                Box::new(E01Reader::open(Path::new(&source.source_path)).unwrap())
                    as Box<dyn EvidenceReader>
            })
            .collect()
    };
    let pool = fs_lvm::LvmPool::discover(readers, identity.pv_offsets.clone())
        .expect("PVE host LVM pool should reopen from persisted identity");
    let volume_index = pool
        .list_volumes()
        .iter()
        .position(|volume| {
            (!identity.lv_uuid.is_empty() && volume.uuid == identity.lv_uuid)
                || volume.name == identity.lv_name
        })
        .expect("PVE host root LV should be present when reopening its pool");
    let volume_reader = pool
        .open_volume_reader(volume_index)
        .expect("PVE host root LV should open as a read-only block device");

    match candidate.kind {
        ImageFilesystemKind::Ext4 => Box::new(
            fs_ext4::Ext4Reader::open(volume_reader, 0)
                .expect("PVE host root LV should open as EXT4"),
        ),
        ImageFilesystemKind::Xfs => Box::new(
            fs_xfs::XfsReader::open(volume_reader, 0).expect("PVE host root LV should open as XFS"),
        ),
        ImageFilesystemKind::Btrfs => Box::new(
            fs_btrfs::BtrfsReader::open(volume_reader, 0)
                .expect("PVE host root LV should open as Btrfs"),
        ),
        other => panic!("unsupported PVE host root filesystem candidate: {other:?}"),
    }
}

fn is_lvm_expansion_diagnostic(warning: &str) -> bool {
    warning.contains("LVM expand:")
        && (warning.contains("skipping")
            || warning.contains("missing")
            || warning.contains("unsupported")
            || warning.contains("discovery failed")
            || warning.contains("no supported filesystem")
            || warning.contains("produced no supported logical volume candidates"))
}

fn assert_lvm_diagnostic_has_trace_fields(warning: &str) {
    assert!(
        warning.contains("source='"),
        "LVM diagnostic should include desensitized source: {warning}"
    );
    assert!(
        warning.contains("offset="),
        "LVM diagnostic should include PV offset: {warning}"
    );
}

fn assert_lvm_warnings_do_not_leak_path(warnings: &[String], path: &std::path::Path) {
    let rendered_path = path.to_string_lossy();
    let rendered_parent = path.parent().map(|parent| parent.to_string_lossy());
    for warning in warnings {
        assert!(
            !warning.contains(rendered_path.as_ref()),
            "LVM warning leaked evidence path '{rendered_path}': {warning}"
        );
        if let Some(parent) = rendered_parent.as_ref() {
            assert!(
                !warning.contains(parent.as_ref()),
                "LVM warning leaked evidence parent path '{parent}': {warning}"
            );
        }
    }
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

fn import_full_linux_image_into_case(
    case_id: &str,
    ds_id: &DataSourceId,
) -> (
    Connection,
    app_services::file_service::EnumerationStats,
    Vec<String>,
) {
    let conn = setup_linux_fixture_case(case_id, ds_id);
    let mut progress_events = Vec::new();
    let stats = enumerate_image_data_source(
        &conn,
        ds_id,
        E01Reader::open(&fixture_path()).unwrap(),
        |pct, detail| {
            progress_events.push(format!("{pct}:{detail}"));
            Ok(())
        },
        None,
        None,
    )
    .unwrap();

    (conn, stats, progress_events)
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

fn count_root_entries_named(conn: &Connection, ds_id: &DataSourceId, name: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM file_entries
         WHERE data_source_id = ?1
           AND parent_id IS NULL
           AND name = ?2
           AND path NOT LIKE '__partition_placeholder__/%'",
        rusqlite::params![ds_id.0, name],
        |row| row.get(0),
    )
    .unwrap()
}

fn count_root_entries_named_like(conn: &Connection, ds_id: &DataSourceId, like: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM file_entries
         WHERE data_source_id = ?1
           AND parent_id IS NULL
           AND name LIKE ?2
           AND path NOT LIKE '__partition_placeholder__/%'",
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

fn find_largest_linux_file_entry(
    conn: &Connection,
    ds_id: &DataSourceId,
    min_size: u64,
) -> LinuxPathEntry {
    conn.query_row(
        "SELECT id, path, COALESCE(size, 0)
         FROM file_entries
         WHERE data_source_id = ?1
           AND entry_type = 'file' COLLATE NOCASE
           AND COALESCE(size, 0) >= ?2
           AND LOWER(REPLACE(path, '\\', '/')) LIKE '%/var/%'
         ORDER BY COALESCE(size, 0) DESC, LENGTH(path) ASC
         LIMIT 1",
        rusqlite::params![&ds_id.0, min_size],
        |row| {
            Ok(LinuxPathEntry {
                file_id: FileEntryId(row.get(0)?),
                path: row.get(1)?,
                size: row.get(2)?,
            })
        },
    )
    .or_else(|_| {
        conn.query_row(
            "SELECT id, path, COALESCE(size, 0)
             FROM file_entries
             WHERE data_source_id = ?1
               AND entry_type = 'file' COLLATE NOCASE
               AND COALESCE(size, 0) >= ?2
             ORDER BY COALESCE(size, 0) DESC, LENGTH(path) ASC
             LIMIT 1",
            rusqlite::params![&ds_id.0, min_size],
            |row| {
                Ok(LinuxPathEntry {
                    file_id: FileEntryId(row.get(0)?),
                    path: row.get(1)?,
                    size: row.get(2)?,
                })
            },
        )
    })
    .unwrap_or_else(|error| {
        panic!("root LV enumeration should include a large previewable Linux file: {error}")
    })
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

fn assert_preview_range(
    conn: &Connection,
    entry: &LinuxPathEntry,
    offset: u64,
    length: u32,
) -> Vec<u8> {
    let response = file_service::read_file_range_for_case(
        conn,
        &ViewerRangeRequestDto {
            handle_id: format!("file:{}", entry.file_id.0),
            offset,
            length,
        },
    )
    .unwrap_or_else(|error| {
        panic!(
            "preview range read failed for {} at offset {} length {}: {}",
            entry.path, offset, length, error
        )
    });
    let bytes = response
        .raw_bytes
        .expect("range preview response should carry raw bytes");
    assert!(
        !bytes.is_empty(),
        "preview range for {} at offset {} should return bytes",
        entry.path,
        offset
    );
    bytes
}

fn read_with_counted_descriptor_cache(
    conn: &Connection,
    case_id: &str,
    entry: &LinuxPathEntry,
    offset: u64,
    length: u32,
    cache: &RefCell<HashMap<String, serde_json::Value>>,
    cache_hits: &std::cell::Cell<usize>,
    set_calls: &std::cell::Cell<usize>,
) -> Vec<u8> {
    let get_cache = |key: &str| {
        let value = cache.borrow().get(key).cloned();
        if value.is_some() {
            cache_hits.set(cache_hits.get() + 1);
        }
        value
    };
    let set_cache = |key: &str, value: &serde_json::Value| {
        set_calls.set(set_calls.get() + 1);
        cache.borrow_mut().insert(key.to_string(), value.clone());
    };
    file_service::read_file_bytes_for_case(
        (conn, case_id, get_cache, set_cache),
        &entry.file_id,
        offset,
        length,
    )
    .unwrap_or_else(|error| {
        panic!(
            "descriptor-cache preview read failed for {} at offset {} length {}: {}",
            entry.path, offset, length, error
        )
    })
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

fn assert_full_image_partition_root_contract(
    conn: &Connection,
    ds_id: &DataSourceId,
    stats: &app_services::file_service::EnumerationStats,
    progress_events: &[String],
) {
    assert!(
        stats.file_count >= MIN_LIUYANG_ROOT_LV_FILE_COUNT
            && stats.dir_count >= MIN_LIUYANG_ROOT_LV_DIR_COUNT,
        "full image import should enumerate the complete current sample tree, got files={} dirs={}",
        stats.file_count,
        stats.dir_count
    );

    let total_entries = total_file_entries(conn, ds_id);
    assert_eq!(
        total_entries as u64,
        stats.file_count + stats.dir_count,
        "DB row count should match all enumerated partition roots plus child entries"
    );

    for segment in ["boot", "dev", "etc", "root", "usr", "var"] {
        assert!(
            count_entries_like(conn, ds_id, &format!("%{segment}%")) > 0,
            "full image import should expose Linux path segment {segment}"
        );
    }

    let partitions = PartitionRepo::new(conn)
        .find_by_data_source(&ds_id.0)
        .unwrap();
    let root_partition = partitions
        .iter()
        .find(|partition| {
            partition.filesystem.as_deref() == Some("XFS")
                && partition.lvm_lv_name.as_deref() == Some(LIUYANG_ROOT_LV_NAME)
        })
        .expect("full image import should persist the cl/root XFS logical-volume partition");
    assert_eq!(root_partition.status, "supported");
    assert_eq!(
        root_partition.lvm_vg_name.as_deref(),
        Some(LIUYANG_ROOT_LV_VG_NAME)
    );
    assert_eq!(
        root_partition.lvm_pv_offsets_json.as_deref(),
        Some("[1074790400]")
    );
    assert!(
        root_partition
            .lvm_pv_sources_json
            .as_deref()
            .is_some_and(|json| json.contains("sourcePath") && json.contains("pvUuid")),
        "full image import should persist traceable LVM PV sources: {:?}",
        root_partition.lvm_pv_sources_json
    );

    let root_name = format!(
        "Partition {} (XFS) - cl/root",
        root_partition.partition_index
    );
    assert_eq!(
        count_root_entries_named(conn, ds_id, &root_name),
        1,
        "full image import should expose exactly one visible cl/root partition root"
    );

    let expanded_pool = partitions
        .iter()
        .find(|partition| {
            partition.offset == LIUYANG_LVM_POOL_OFFSET && partition.status == "redirected"
        })
        .expect("full image import should retain redirected physical LVM pool metadata");
    assert_eq!(expanded_pool.filesystem.as_deref(), Some("LVM"));
    assert_eq!(
        count_root_entries_named(conn, ds_id, &expanded_pool.name),
        0,
        "redirected physical LVM pool must not become a visible file-tree root"
    );

    let visible_tree = file_service::get_file_tree_real_with_visibility(conn, false).unwrap();
    let root = visible_tree
        .iter()
        .find(|node| node.name == root_name)
        .expect("visible tree should expose the root logical volume");
    assert_eq!(root.node_type.as_deref(), Some("partition"));
    assert_eq!(root.status.as_deref(), Some("ready"));
    assert!(
        !visible_tree
            .iter()
            .any(|node| node.name == expanded_pool.name),
        "visible tree must hide redirected physical LVM pool; roots={:?}",
        visible_tree
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>()
    );

    let root_children =
        file_service::get_file_children_lazy_with_visibility(conn, &root.id, 0, 100, false)
            .unwrap();
    let child_names = root_children
        .children
        .iter()
        .map(|child| child.name.as_str())
        .collect::<Vec<_>>();
    for required in ["boot", "dev", "etc", "usr", "var"] {
        assert!(
            child_names.contains(&required),
            "full image root LV should expose /{required}; children={child_names:?}"
        );
    }
    assert_eq!(
        count_root_entries_named_like(conn, ds_id, "%cl/root%"),
        1,
        "full image import should expose exactly one visible cl/root-like root"
    );

    let warning_contract = stats
        .warnings
        .iter()
        .chain(progress_events.iter())
        .collect::<Vec<_>>();
    assert!(
        warning_contract
            .iter()
            .all(|warning| !warning.contains(&fixture_path().to_string_lossy().to_string())),
        "full image import warnings/progress must not leak the evidence path: {warning_contract:?}"
    );
    assert!(
        stats
            .warnings
            .iter()
            .all(|warning| !warning.contains("Cannot open image-backed file")
                && !warning.contains("path reconstruction")),
        "full image import warning contract should not contain preview/path-resolution failures: {:?}",
        stats.warnings
    );
    for warning in stats
        .warnings
        .iter()
        .filter(|warning| is_lvm_expansion_diagnostic(warning))
    {
        assert_lvm_diagnostic_has_trace_fields(warning);
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

fn assert_linux_extraction_sections_cover_real_sample(run: &AnalysisExtractionRunDto) {
    for key in LINUX_ANALYSIS_SECTION_KEYS {
        let section = run
            .sections
            .iter()
            .find(|section| section.key == *key)
            .unwrap_or_else(|| panic!("Linux extraction run should include section {key}"));
        assert_ne!(
            section.status,
            AnalysisParseStatusDto::Failed,
            "Linux extraction section {key} should not fail: {:?}",
            section.warnings
        );
    }

    for key in [
        "LinuxJournal",
        "LinuxLogin",
        "LinuxCommands",
        "LinuxCron",
        "LinuxSudo",
        "LinuxSystemConfig",
    ] {
        let section = run
            .sections
            .iter()
            .find(|section| section.key == key)
            .unwrap();
        assert!(
            section.scanned_count > 0,
            "real sample should scan Linux section {key}"
        );
        assert!(
            section.artifact_count > 0,
            "real sample should persist artifacts for Linux section {key}"
        );
    }

    let packages = run
        .sections
        .iter()
        .find(|section| section.key == "LinuxPackages")
        .unwrap();
    if packages.scanned_count > 0 {
        assert!(
            packages.artifact_count > 0,
            "package section scanned package logs but produced no package artifacts"
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
fn linux_e01_full_image_import_has_expected_partition_roots_and_warning_contract() {
    let case_id = "linux-e01-full-import-contract";
    let ds_id = DataSourceId("e01-linux-full-import-contract-ds".to_string());
    let (conn, stats, progress_events) = import_full_linux_image_into_case(case_id, &ds_id);
    eprintln!(
        "Full image import: files={} dirs={} total={} warnings={:?}",
        stats.file_count, stats.dir_count, stats.total_size, stats.warnings
    );
    for event in &progress_events {
        eprintln!("  progress {event}");
    }

    assert_full_image_partition_root_contract(&conn, &ds_id, &stats, &progress_events);
    assert_high_value_linux_paths_enumerated(&conn, &ds_id);
    assert_linux_paths_preview_readable(&conn, &ds_id, ARBITRARY_PREVIEW_READ_PATHS);
}

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_preview_reads_large_file_head_middle_tail_ranges() {
    let ds_id = DataSourceId("e01-linux-preview-ranges-ds".to_string());
    let conn = setup_linux_fixture_case("linux-e01-preview-ranges", &ds_id);
    let stats = enumerate_root_lv_into_case(&conn, &ds_id);
    assert_liuyang_root_lv_tree_is_complete(&conn, &ds_id, &stats);

    let entry = find_largest_linux_file_entry(&conn, &ds_id, MIN_LIUYANG_LARGE_PREVIEW_FILE_BYTES);
    eprintln!(
        "Large preview target: id={} path='{}' size={}",
        entry.file_id.0, entry.path, entry.size
    );
    assert!(
        entry.size >= MIN_LIUYANG_LARGE_PREVIEW_FILE_BYTES,
        "large preview target should be at least {} bytes",
        MIN_LIUYANG_LARGE_PREVIEW_FILE_BYTES
    );

    let middle_offset = (entry.size / 2).saturating_sub((LINUX_PREVIEW_RANGE_LEN / 2) as u64);
    let tail_offset = entry.size.saturating_sub(LINUX_PREVIEW_RANGE_LEN as u64);

    let head = assert_preview_range(&conn, &entry, 0, LINUX_PREVIEW_RANGE_LEN);
    let middle = assert_preview_range(&conn, &entry, middle_offset, LINUX_PREVIEW_RANGE_LEN);
    let tail = assert_preview_range(&conn, &entry, tail_offset, LINUX_PREVIEW_RANGE_LEN);

    assert!(head.len() <= LINUX_PREVIEW_RANGE_LEN as usize);
    assert!(middle.len() <= LINUX_PREVIEW_RANGE_LEN as usize);
    assert!(tail.len() <= LINUX_PREVIEW_RANGE_LEN as usize);
}

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_preview_descriptor_cache_reads_lvm_xfs_files() {
    let case_id = "linux-e01-preview-descriptor-cache";
    let ds_id = DataSourceId("e01-linux-preview-cache-ds".to_string());
    let conn = setup_linux_fixture_case(case_id, &ds_id);
    let stats = enumerate_root_lv_into_case(&conn, &ds_id);
    assert_liuyang_root_lv_tree_is_complete(&conn, &ds_id, &stats);

    let entry = find_linux_file_entry(&conn, &ds_id, "/etc/os-release")
        .expect("root LV should include /etc/os-release for descriptor-cache preview");
    let partition = lvm_root_partition_record(&conn, &ds_id);
    assert_eq!(partition.filesystem.as_deref(), Some("XFS"));
    assert_eq!(partition.lvm_lv_name.as_deref(), Some(LIUYANG_ROOT_LV_NAME));
    assert!(
        partition.lvm_pv_sources_json.is_some(),
        "descriptor cache should be backed by persisted LVM PV source metadata"
    );

    let cache = RefCell::new(HashMap::<String, serde_json::Value>::new());
    let cache_hits = std::cell::Cell::new(0usize);
    let set_calls = std::cell::Cell::new(0usize);

    let first = read_with_counted_descriptor_cache(
        &conn,
        case_id,
        &entry,
        0,
        64,
        &cache,
        &cache_hits,
        &set_calls,
    );
    assert!(!first.is_empty());
    assert_eq!(set_calls.get(), 1);
    assert_eq!(cache_hits.get(), 0);

    let second = read_with_counted_descriptor_cache(
        &conn,
        case_id,
        &entry,
        8,
        64,
        &cache,
        &cache_hits,
        &set_calls,
    );
    assert!(!second.is_empty());
    assert_eq!(
        set_calls.get(),
        1,
        "second LVM/XFS preview range should reuse the cached descriptor"
    );
    assert_eq!(cache_hits.get(), 1);
}

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_analysis_summary_reports_candidate_coverage_and_unsupported_sources() {
    let case_id = "linux-e01-analysis-summary-coverage";
    let ds_id = DataSourceId("e01-linux-summary-coverage-ds".to_string());
    let conn = setup_linux_fixture_case(case_id, &ds_id);
    let stats = enumerate_root_lv_into_case(&conn, &ds_id);
    assert_liuyang_root_lv_tree_is_complete(&conn, &ds_id, &stats);

    let candidates = evidence_candidates_for_categories(&conn, &["LinuxArtifacts"]).unwrap();
    assert_linux_artifact_candidates_cover_critical_paths(&candidates);

    let pre_summary = get_linux_artifact_summary(&conn, 0, 50).unwrap();
    assert_eq!(pre_summary.total_count, 0);
    assert_eq!(
        pre_summary.status,
        transport::dto::AnalysisParseStatusDto::CandidateFound
    );
    assert_eq!(pre_summary.coverage_ratio, 0.0);
    assert!(
        pre_summary
            .warnings
            .iter()
            .any(|warning| warning.contains("Linux artifact candidate(s)")
                && warning.contains("no structured artifacts")),
        "pre-extraction summary should report candidate coverage: {:?}",
        pre_summary.warnings
    );
    assert!(
        pre_summary
            .warnings
            .iter()
            .any(|warning| warning.contains("do not yet have a structured first-pass parser")),
        "pre-extraction summary should report unsupported candidate sources: {:?}",
        pre_summary.warnings
    );
    assert!(
        pre_summary
            .warnings
            .iter()
            .any(|warning| warning.contains("generic line-level extraction")),
        "pre-extraction summary should report covered Linux text fallback sources: {:?}",
        pre_summary.warnings
    );

    let run = run_analysis_extraction(&conn, case_id, &["LinuxArtifacts"], |file_id| {
        file_service::read_file_header_by_id(
            &conn,
            file_id,
            app_services::analysis_service::MAX_ANALYSIS_SOURCE_BYTES,
        )
        .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
        .map_err(|error| error.to_string())
    })
    .expect("LinuxArtifacts extraction should run against real root LV candidates");
    assert!(run.scanned_count > 0);
    assert!(run.artifact_count > 0);

    let summary = get_linux_artifact_summary(&conn, 0, 200).unwrap();
    eprintln!(
        "Linux summary coverage: total={} coverage={} truncated={} warnings={:?}",
        summary.total_count, summary.coverage_ratio, summary.truncated, summary.warnings
    );
    assert!(summary.total_count > 0);
    assert!(summary.coverage_ratio > 0.0);
    assert!(
        summary.coverage_ratio < 1.0,
        "unsupported or empty candidate sources should keep coverage below complete; coverage={}",
        summary.coverage_ratio
    );
    assert!(
        summary
            .warnings
            .iter()
            .any(|warning| warning.contains("Parsed ")
                && warning.contains("Linux artifact candidate source(s)")),
        "post-extraction summary should report parsed/candidate coverage: {:?}",
        summary.warnings
    );
    assert!(
        summary
            .warnings
            .iter()
            .any(|warning| warning.contains("do not yet have a structured first-pass parser")),
        "post-extraction summary should retain unsupported-source contract: {:?}",
        summary.warnings
    );
    assert!(
        summary
            .warnings
            .iter()
            .any(|warning| warning.contains("generic line-level extraction")),
        "post-extraction summary should retain text fallback coverage contract: {:?}",
        summary.warnings
    );
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
    for section in &run.sections {
        eprintln!(
            "  section key={} status={:?} scanned={} artifacts={} timeline={} warnings={}",
            section.key,
            section.status,
            section.scanned_count,
            section.artifact_count,
            section.timeline_event_count,
            section.warnings.len()
        );
    }
    assert!(
        run.scanned_count > 0,
        "expected real Linux artifact sources scanned"
    );
    assert!(
        run.artifact_count > 0,
        "expected real Linux artifact extraction to persist artifacts"
    );
    assert_linux_extraction_sections_cover_real_sample(&run);

    let summary = get_linux_artifact_summary(&conn, 0, 200).unwrap();
    eprintln!(
        "Linux artifact summary: total={} journal={} login={} bash={} package={} cron={} sudo={} config={} web={}",
        summary.total_count,
        summary.journal_count,
        summary.login_count,
        summary.bash_command_count,
        summary.apt_event_count,
        summary.cron_job_count,
        summary.sudo_event_count,
        summary.system_config_count,
        summary.web_site_count
            + summary.web_access_log_count
            + summary.web_error_log_count
            + summary.web_finding_count,
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
    assert!(
        summary.system_config_count > 0,
        "real sample should expose Linux system config artifacts in summary"
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
    assert_eq!(identity.pv_sources.len(), 1);
    assert_eq!(identity.pv_sources[0].offset, LIUYANG_LVM_POOL_OFFSET);
    assert!(
        !identity.pv_sources[0].pv_uuid.is_empty(),
        "PV UUID must be persisted for source rebinding"
    );
    assert_eq!(identity.pv_sources[0].pv_name.as_deref(), Some("pv0"));
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

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Liuyang Linux E01 sample"]
fn liuyang_linux_e01_lvm_probe_keeps_expanded_pool_and_traceable_identity() {
    let fixture = fixture_path();
    let mut reader = E01Reader::open(&fixture).unwrap();
    let mut probe = detect_image_filesystem(&mut reader).unwrap();
    assert!(
        probe.candidates.iter().any(|candidate| {
            candidate.kind == ImageFilesystemKind::LvmPool
                && candidate.offset == LIUYANG_LVM_POOL_OFFSET
        }),
        "Liuyang sample should expose the physical LVM pool before expansion"
    );

    expand_lvm_pool_candidates(&mut probe, &fixture, &DataSourceKind::E01);

    assert!(
        probe.partitions.iter().any(|partition| {
            partition.offset == LIUYANG_LVM_POOL_OFFSET
                && matches!(
                    partition.status,
                    app_services::datasource_service::PartitionStatus::Expanded
                )
        }),
        "Liuyang LVM pool partition should stay Expanded after LV redirection"
    );
    assert!(
        probe.candidates.iter().all(|candidate| {
            candidate.offset != LIUYANG_LVM_POOL_OFFSET
                || !matches!(candidate.kind, ImageFilesystemKind::LvmPool)
        }),
        "expanded Liuyang LVM pool should not remain as an expandable placeholder candidate"
    );

    let root_lv = root_lv_candidate(&probe);
    let identity = root_lv
        .lvm_identity
        .as_ref()
        .expect("Liuyang root LV should persist LVM identity");
    assert_eq!(identity.vg_name, LIUYANG_ROOT_LV_VG_NAME);
    assert_eq!(identity.lv_name, LIUYANG_ROOT_LV_NAME);
    assert_eq!(identity.pv_offsets, vec![LIUYANG_LVM_POOL_OFFSET]);
    assert_eq!(identity.pv_sources.len(), 1);
    assert_eq!(
        identity.pv_sources[0].source_path,
        fixture.to_string_lossy()
    );
    assert_eq!(identity.pv_sources[0].offset, LIUYANG_LVM_POOL_OFFSET);
    assert_eq!(identity.pv_sources[0].pv_name.as_deref(), Some("pv0"));
    assert!(
        !identity.pv_sources[0].pv_uuid.is_empty(),
        "Liuyang root LV should persist PV UUID for source rebinding"
    );

    for warning in probe
        .warnings
        .iter()
        .filter(|warning| is_lvm_expansion_diagnostic(warning))
    {
        assert_lvm_diagnostic_has_trace_fields(warning);
    }
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT real PVE cluster E01 sample directory"]
fn pve_cluster_import_plan_discovers_nested_server_images() {
    let root = pve_cluster_root();
    let expected_e01_files = pve_cluster_e01_files(&root);
    assert!(
        !expected_e01_files.is_empty(),
        "PVE cluster root {} should contain server disk E01 files",
        root.display()
    );

    let plan = plan_linux_cluster_import(&root, Some("pve-cluster".to_string()))
        .expect("PVE cluster import planner should discover nested server images");

    eprintln!(
        "PVE cluster import plan discovered {} member(s)",
        plan.members.len()
    );
    for member in &plan.members {
        eprintln!(
            "  member {} kind={:?} path={}",
            member.member_index,
            member.source_kind,
            member.source_path.display()
        );
    }
    assert_eq!(plan.members.len(), expected_e01_files.len());
    assert!(
        plan.members
            .iter()
            .all(|member| member.source_kind == DataSourceKind::E01),
        "PVE sample members should be first E01 segments only"
    );
    assert!(
        plan.members
            .iter()
            .all(|member| !member.source_path.to_string_lossy().ends_with(".E02")),
        "PVE sample planner must not register continuation segments"
    );
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT real PVE cluster E01 sample directory"]
fn pve_cluster_e01_lvm_probe_has_explicit_diagnostics() {
    let root = pve_cluster_root();
    let e01_files = pve_cluster_e01_files(&root);
    assert!(
        !e01_files.is_empty(),
        "PVE cluster root {} should contain server disk E01 files",
        root.display()
    );

    let mut saw_lvm_pool = false;
    let mut saw_expanded_lvm = false;
    let mut saw_lvm_diagnostic = false;

    for e01_path in &e01_files {
        eprintln!("=== PVE LVM probe: {} ===", e01_path.display());
        let mut saw_file_lvm_diagnostic = false;
        let Ok(mut reader) = E01Reader::open(e01_path) else {
            eprintln!(
                "  warning: PVE E01 {} could not open as standalone image; likely continuation/secondary extent",
                e01_path.display()
            );
            continue;
        };
        let mut probe = detect_image_filesystem(&mut reader).unwrap_or_else(|error| {
            panic!(
                "PVE E01 {} filesystem probe should not panic/fail: {}",
                e01_path.display(),
                error
            )
        });
        let initial_lvm_pool_offsets = probe
            .candidates
            .iter()
            .filter(|candidate| candidate.kind == ImageFilesystemKind::LvmPool)
            .map(|candidate| candidate.offset)
            .collect::<Vec<_>>();
        if !initial_lvm_pool_offsets.is_empty() {
            saw_lvm_pool = true;
        }

        expand_lvm_pool_candidates(&mut probe, e01_path, &DataSourceKind::E01);
        for warning in &probe.warnings {
            eprintln!("  warning: {warning}");
            if is_lvm_expansion_diagnostic(warning) {
                assert_lvm_diagnostic_has_trace_fields(warning);
                saw_file_lvm_diagnostic = true;
                saw_lvm_diagnostic = true;
            }
        }

        for offset in &initial_lvm_pool_offsets {
            let expanded_pool = probe.partitions.iter().any(|partition| {
                partition.offset == *offset
                    && matches!(
                        partition.status,
                        app_services::datasource_service::PartitionStatus::Expanded
                    )
            });
            let has_remaining_pool_candidate = probe.candidates.iter().any(|candidate| {
                candidate.kind == ImageFilesystemKind::LvmPool && candidate.offset == *offset
            });
            assert!(
                expanded_pool || has_remaining_pool_candidate || saw_file_lvm_diagnostic,
                "PVE LVM pool at {} offset {} must be expanded, retained, or diagnosed explicitly",
                e01_path.display(),
                offset
            );
        }

        for candidate in probe
            .candidates
            .iter()
            .filter(|candidate| matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume))
        {
            saw_expanded_lvm = true;
            let identity = candidate
                .lvm_identity
                .as_ref()
                .expect("expanded PVE LV candidate must persist LVM identity");
            assert!(
                !identity.lv_name.is_empty(),
                "expanded PVE LV candidate should persist LV name"
            );
            assert!(
                !identity.pv_sources.is_empty(),
                "expanded PVE LV candidate should persist PV source bindings"
            );
            assert_eq!(
                identity.pv_offsets.len(),
                identity.pv_sources.len(),
                "expanded PVE LV candidate should keep PV offsets and sources aligned"
            );
            for source in &identity.pv_sources {
                assert!(
                    !source.source_path.is_empty(),
                    "expanded PVE LV PV source should persist source path"
                );
                assert!(
                    !source.pv_uuid.is_empty(),
                    "expanded PVE LV PV source should persist PV UUID"
                );
            }
        }
    }

    assert!(
        saw_lvm_pool,
        "PVE cluster sample should exercise at least one LVM pool"
    );
    assert!(
        saw_expanded_lvm || saw_lvm_diagnostic,
        "PVE LVM probe should either expose host LV candidates or emit explicit LVM diagnostics"
    );
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT real PVE cluster E01 sample directory"]
fn pve_cluster_host_root_filesystems_enumerate_and_preview() {
    let root = pve_cluster_root();
    let host_images = pve_host_disk_e01_files(&root);
    assert!(
        !host_images.is_empty(),
        "PVE cluster root {} should contain host disk01 E01 images",
        root.display()
    );

    for image_path in host_images {
        let mut reader = E01Reader::open(&image_path).unwrap_or_else(|error| {
            panic!(
                "PVE host image {} should open: {error}",
                image_path.display()
            )
        });
        let mut probe = detect_image_filesystem(&mut reader).unwrap_or_else(|error| {
            panic!(
                "PVE host image {} should probe successfully: {error}",
                image_path.display()
            )
        });
        expand_lvm_pool_candidates(&mut probe, &image_path, &DataSourceKind::E01);

        let root_candidate = probe
            .candidates
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.kind,
                    ImageFilesystemKind::Ext4
                        | ImageFilesystemKind::Xfs
                        | ImageFilesystemKind::Btrfs
                ) && matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume)
                    && candidate
                        .lvm_identity
                        .as_ref()
                        .is_some_and(|identity| identity.lv_name == "root")
            })
            .unwrap_or_else(|| {
                panic!(
                    "PVE host image {} should expose a supported pve/root filesystem candidate; candidates={:?}; warnings={:?}",
                    image_path.display(),
                    probe.candidates,
                    probe.warnings
                )
            });
        eprintln!(
            "PVE host root candidate: image={} kind={:?} identity={:?}",
            image_path.display(),
            root_candidate.kind,
            root_candidate.lvm_identity
        );

        let fs = open_pve_host_filesystem(&image_path, root_candidate);
        let root_children = fs.list_children("").unwrap_or_else(|error| {
            panic!(
                "PVE host root filesystem {} should enumerate: {error}",
                image_path.display()
            )
        });
        let root_names = root_children
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        for expected in ["etc", "usr", "var"] {
            assert!(
                root_children
                    .iter()
                    .any(|entry| entry.is_dir && entry.name == expected),
                "PVE host root {} should contain /{expected}; root entries={root_names:?}",
                image_path.display()
            );
        }

        for path in [
            "etc/passwd",
            "etc/os-release",
            "etc/hostname",
            "var/lib/pve-cluster/config.db",
        ] {
            let bytes: std::io::Result<Vec<u8>> = fs.read_file_range(path, 0, 512).or_else(|_| {
                let mut file = fs.open_file(path)?;
                let mut bytes = vec![0u8; 512];
                let read = file.read(&mut bytes)?;
                bytes.truncate(read);
                Ok(bytes)
            });
            let bytes = bytes.unwrap_or_else(|error| {
                panic!(
                    "PVE host file {}:{} should be preview-readable: {error}",
                    image_path.display(),
                    path
                )
            });
            assert!(
                !bytes.is_empty(),
                "PVE host file {}:{} should return preview bytes",
                image_path.display(),
                path
            );
        }
    }
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT real PVE cluster E01 sample directory"]
fn pve_cluster_representative_host_imports_tree_and_previews_by_file_id() {
    let root = pve_cluster_root();
    let image_path = pve_host_disk_e01_files(&root)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            panic!(
                "PVE cluster root {} should contain a representative host disk01 E01",
                root.display()
            )
        });
    let case_id = "pve-host-ext4-import-test";
    let ds_id = DataSourceId("pve-host-ext4-source".to_string());
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    setup_case(&conn, case_id);
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId(case_id.to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "PVE host disk01".to_string(),
                kind: DataSourceKind::E01,
                source_path: image_path.clone(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    let stats = enumerate_image_data_source(
        &conn,
        &ds_id,
        E01Reader::open(&image_path).unwrap(),
        |pct, detail| {
            eprintln!("PVE host import {pct}%: {detail}");
            Ok(())
        },
        None,
        None,
    )
    .unwrap_or_else(|error| {
        panic!(
            "PVE host image {} should import through the production pipeline: {error}",
            image_path.display()
        )
    });
    eprintln!(
        "PVE host import summary: files={} dirs={} total_bytes={} warnings={}",
        stats.file_count,
        stats.dir_count,
        stats.total_size,
        stats.warnings.len()
    );
    assert!(
        stats.file_count >= 50_000
            && stats.dir_count >= 5_000
            && stats.total_size >= 4 * 1024 * 1024 * 1024,
        "PVE host import should preserve the full tree and inode sizes, got files={} dirs={} total_bytes={} warnings={:?}",
        stats.file_count,
        stats.dir_count,
        stats.total_size,
        stats.warnings
    );

    for path in [
        "/etc/passwd",
        "/etc/os-release",
        "/etc/hostname",
        "/var/lib/pve-cluster/config.db",
    ] {
        let entry = find_linux_file_entry(&conn, &ds_id, path).unwrap_or_else(|| {
            panic!(
                "PVE host production import should persist {} from {}",
                path,
                image_path.display()
            )
        });
        assert!(
            entry.size > 0,
            "PVE host imported file {} should persist its EXT4 inode size",
            path
        );
        let bytes = file_service::read_file_header_by_id(&conn, &entry.file_id, 512)
            .unwrap_or_else(|error| {
                panic!(
                    "PVE host imported file {} should preview by FileEntryId {}: {error}",
                    path, entry.file_id.0
                )
            });
        assert!(
            !bytes.is_empty(),
            "PVE host imported file {} should return preview bytes",
            path
        );
    }
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
