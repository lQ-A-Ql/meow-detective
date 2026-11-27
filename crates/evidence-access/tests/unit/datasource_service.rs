use super::lvm::source_identity::lvm_pv_source_key;
use super::*;
use domain::{CaseId, CaseMeta, DataSourceHashStatus, DataSourceKind, DataSourceProvenanceStatus};
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use tempfile::TempDir;
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
    assert_eq!(stored.provenance.hash_status, DataSourceHashStatus::Unknown);
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
fn detect_image_filesystem_detects_raw_lvm_pv_at_offset_zero() {
    let (pv0, _pv1) = native_multi_pv_lvm_disks();
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

#[test]
fn collection_plan_groups_native_pvs() {
    let tmp = TempDir::new().unwrap();
    let primary_path = tmp.path().join("pv0.raw");
    let extra_path = tmp.path().join("pv1.raw");
    let (pv0, pv1) = native_multi_pv_lvm_disks();
    let uuid0 = fs_lvm::label::parse_pv_label(&mut std::io::Cursor::new(&pv0), 0)
        .unwrap()
        .pv_uuid;
    let uuid1 = fs_lvm::label::parse_pv_label(&mut std::io::Cursor::new(&pv1), 0)
        .unwrap()
        .pv_uuid;
    std::fs::write(&primary_path, pv0).unwrap();
    std::fs::write(&extra_path, pv1).unwrap();

    let plan = ImageCollectionPlan::prepare(
        &[
            LvmDiscoverySource::new(&primary_path, DataSourceKind::Raw),
            LvmDiscoverySource::new(&extra_path, DataSourceKind::Raw),
        ],
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let probe = plan
        .member_probe(0, &primary_path, &DataSourceKind::Raw)
        .unwrap();

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
    assert_eq!(lvm_candidate.kind, ImageFilesystemKind::Ext4);
    let identity = lvm_candidate.lvm_identity.as_ref().unwrap();
    assert!(!identity.vg_name.is_empty());
    assert_eq!(identity.lv_name, "logical");
    assert_eq!(identity.pv_sources.len(), 2);
    assert_eq!(
        std::path::Path::new(&identity.pv_sources[0].source_path),
        primary_path.canonicalize().unwrap()
    );
    assert_eq!(
        identity.pv_sources[0].source_kind,
        Some(DataSourceKind::Raw)
    );
    assert_eq!(identity.pv_sources[0].pv_uuid, uuid0);
    assert_eq!(identity.pv_sources[0].pv_name.as_deref(), Some("pv0"));
    assert_eq!(
        std::path::Path::new(&identity.pv_sources[1].source_path),
        extra_path.canonicalize().unwrap()
    );
    assert_eq!(
        identity.pv_sources[1].source_kind,
        Some(DataSourceKind::Raw)
    );
    assert_eq!(identity.pv_sources[1].pv_uuid, uuid1);
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
fn source_local_expansion_requires_all_native_pvs() {
    let tmp = TempDir::new().unwrap();
    let primary_path = tmp.path().join("pv0.raw");
    let (_, pv1) = native_multi_pv_lvm_disks();
    std::fs::write(&primary_path, pv1).unwrap();

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

fn native_multi_pv_lvm_disks() -> (Vec<u8>, Vec<u8>) {
    use std::io::Read;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/lvm-native/shared-ext4");
    let mut images = Vec::new();
    for index in 0..2 {
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(
            std::fs::File::open(root.join(format!("pv-{index}.raw.gz"))).unwrap(),
        )
        .read_to_end(&mut bytes)
        .unwrap();
        images.push(bytes);
    }
    (images.remove(0), images.remove(0))
}
