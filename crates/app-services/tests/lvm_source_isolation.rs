use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::datasource_service::{self, ImageFilesystemSource, LvmDiscoverySource};
use app_services::import_pipeline::{execute_import_job, ImportJobOptions};
use domain::{DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, job_repo::JobRepo, partition_repo::PartitionRepo,
};

const SECTOR_SIZE: u64 = 512;
const PV_SIZE: u64 = 2_097_152;
const PV_OFFSET: u64 = 1_048_576;
const DATA_AREA_START: u64 = 2_560;
const PRIMARY_PV_UUID: &str = "00000000000000000000000000000000";
const SUPPLEMENTARY_PV_UUID: &str = "11111111111111111111111111111111";

#[test]
fn source_local_expansion_does_not_scan_a_neighboring_pv_image() {
    let temp = tempfile::TempDir::new().expect("create temporary LVM fixture directory");
    let primary_path = temp.path().join("primary.raw");
    let neighboring_path = temp.path().join("neighbor.raw");
    write_multi_pv_fixture(&primary_path, &neighboring_path);

    let mut reader =
        evidence_core::RawImageReader::open(&primary_path).expect("open primary synthetic image");
    let mut probe = datasource_service::detect_image_filesystem(&mut reader)
        .expect("probe primary synthetic image");

    datasource_service::expand_lvm_pool_candidates(&mut probe, &primary_path, &DataSourceKind::Raw);

    assert!(
        probe
            .candidates
            .iter()
            .all(|candidate| candidate.source != ImageFilesystemSource::LvmLogicalVolume),
        "source-local discovery must not complete a VG from a neighboring image"
    );
    assert!(
        probe
            .warnings
            .iter()
            .any(|warning| warning.contains("skipping incomplete")),
        "the missing PV must remain explicit: {:?}",
        probe.warnings
    );
    assert!(
        probe
            .warnings
            .iter()
            .all(|warning| !warning.contains(neighboring_path.to_string_lossy().as_ref())),
        "source-local diagnostics must not reveal or consume the neighboring source path"
    );
}

#[test]
fn ordinary_import_does_not_consume_a_case_registered_supplementary_pv() {
    let temp = tempfile::TempDir::new().expect("create temporary LVM fixture directory");
    let primary_path = temp.path().join("primary.raw");
    let supplementary_path = temp.path().join("other-source.raw");
    write_multi_pv_fixture(&primary_path, &supplementary_path);

    assert_explicit_multi_source_discovery_can_complete_the_volume_group(
        &primary_path,
        &supplementary_path,
    );

    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "lvm-source-isolation",
        Some("test"),
    )
    .expect("create test case");
    let cancel = Arc::new(AtomicBool::new(false));

    active
        .with_conn(|case_conn| {
            let supplementary = datasource_service::attach_data_source(
                case_conn,
                &active.meta.id,
                "registered unrelated source",
                &supplementary_path,
                DataSourceKind::Raw,
                DataSourcePlatform::Linux,
            )
            .expect("register supplementary source in the same case");
            let job_id = JobRepo::new(case_conn)
                .create(&active.meta.id.0, "LVM source-isolation import")
                .expect("create import job");
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &primary_path.to_string_lossy(),
                DataSourcePlatform::Linux,
            )
            .expect("prepare primary import");

            // The synthetic XFS marker is intentionally minimal, so enumeration may fail.
            // Partition metadata is persisted before reader construction either way.
            let _result = execute_import_job(
                case_conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job_id,
                ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
                },
            );

            let primary = DataSourceRepo::new(case_conn)
                .find_by_case(&active.meta.id)
                .expect("list case data sources")
                .into_iter()
                .find(|source| source.source_path == primary_path)
                .expect("primary import should remain registered after probe");
            assert_ne!(primary.id, supplementary.id);

            let source_conn =
                app_services::source_db::open_source_db(&active.case_root, &primary.id)
                    .expect("open primary source database");
            let partitions = PartitionRepo::new(&source_conn)
                .find_by_data_source(&primary.id.0)
                .expect("read persisted primary partitions");
            assert!(
                !partitions.is_empty(),
                "primary probe must persist its partition"
            );
            assert!(
                partitions.iter().all(|partition| {
                    partition.lvm_lv_name.is_none()
                        && partition
                            .lvm_pv_sources_json
                            .as_deref()
                            .is_none_or(|sources| {
                                !sources.contains(SUPPLEMENTARY_PV_UUID)
                                    && !sources
                                        .contains(supplementary_path.to_string_lossy().as_ref())
                            })
                }),
                "ordinary import must not persist another data source as an LVM PV: {partitions:?}"
            );

            Ok(())
        })
        .expect("inspect source-isolated import");
}

fn assert_explicit_multi_source_discovery_can_complete_the_volume_group(
    primary_path: &std::path::Path,
    supplementary_path: &std::path::Path,
) {
    let mut reader =
        evidence_core::RawImageReader::open(primary_path).expect("open primary synthetic image");
    let mut probe = datasource_service::detect_image_filesystem(&mut reader)
        .expect("probe primary synthetic image");
    datasource_service::expand_lvm_pool_candidates_with_sources(
        &mut probe,
        primary_path,
        &DataSourceKind::Raw,
        &[LvmDiscoverySource::new(
            supplementary_path,
            DataSourceKind::Raw,
        )],
    );

    let identity = probe
        .candidates
        .iter()
        .find(|candidate| candidate.source == ImageFilesystemSource::LvmLogicalVolume)
        .and_then(|candidate| candidate.lvm_identity.as_ref())
        .unwrap_or_else(|| {
            panic!(
                "fixture must expose the contamination risk when sources are explicitly combined: {:?}",
                probe.warnings
            )
        });
    assert_eq!(identity.pv_sources.len(), 2);
    assert!(identity
        .pv_sources
        .iter()
        .any(|source| source.pv_uuid == SUPPLEMENTARY_PV_UUID));
}

fn write_multi_pv_fixture(primary_path: &std::path::Path, supplementary_path: &std::path::Path) {
    let metadata = format!(
        r#"test_vg {{
    id = "vg-multi-pv-1234"
    seqno = 2
    extent_size = 1

    physical_volumes {{
        pv0 {{
            id = "{PRIMARY_PV_UUID}"
            device = "/dev/sda1"
            pe_start = 5
            pe_count = 16
        }}
        pv1 {{
            id = "{SUPPLEMENTARY_PV_UUID}"
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
"#
    );

    let mut primary_pv = vec![0u8; PV_SIZE as usize];
    let mut supplementary_pv = vec![0u8; PV_SIZE as usize];
    write_pv_label(&mut primary_pv, PRIMARY_PV_UUID);
    write_pv_label(&mut supplementary_pv, SUPPLEMENTARY_PV_UUID);
    write_lvm_metadata(&mut primary_pv, &metadata);
    primary_pv[DATA_AREA_START as usize..DATA_AREA_START as usize + 4].copy_from_slice(b"XFSB");

    std::fs::write(primary_path, wrap_in_mbr(&primary_pv)).expect("write primary fixture");
    std::fs::write(supplementary_path, wrap_in_mbr(&supplementary_pv))
        .expect("write supplementary fixture");
}

fn wrap_in_mbr(pv: &[u8]) -> Vec<u8> {
    let mut image = vec![0u8; PV_OFFSET as usize + pv.len()];
    image[PV_OFFSET as usize..].copy_from_slice(pv);
    let entry = &mut image[446..462];
    entry[4] = 0x8e;
    entry[8..12].copy_from_slice(&((PV_OFFSET / SECTOR_SIZE) as u32).to_le_bytes());
    entry[12..16].copy_from_slice(&((pv.len() as u64 / SECTOR_SIZE) as u32).to_le_bytes());
    image[510] = 0x55;
    image[511] = 0xaa;
    image
}

fn write_pv_label(pv: &mut [u8], pv_uuid: &str) {
    let pv_size = pv.len() as u64;
    let sector = &mut pv[512..1024];
    sector[0..8].copy_from_slice(b"LABELONE");
    sector[8..16].copy_from_slice(&1u64.to_le_bytes());
    sector[20..24].copy_from_slice(&32u32.to_le_bytes());
    sector[24..32].copy_from_slice(b"LVM2 001");
    sector[32..64].copy_from_slice(format!("{pv_uuid:32}").as_bytes());
    sector[64..72].copy_from_slice(&pv_size.to_le_bytes());
    sector[72..80].copy_from_slice(&DATA_AREA_START.to_le_bytes());
    sector[80..88].copy_from_slice(&(pv_size - DATA_AREA_START).to_le_bytes());
    sector[104..112].copy_from_slice(&1024u64.to_le_bytes());
    sector[112..120].copy_from_slice(&(4 * SECTOR_SIZE).to_le_bytes());
    let crc = fs_lvm::crc::lvm_crc32(&sector[20..512]);
    sector[16..20].copy_from_slice(&crc.to_le_bytes());
}

fn write_lvm_metadata(pv: &mut [u8], metadata: &str) {
    let bytes = metadata.as_bytes();
    let text_offset = 1536usize;
    let text_end = text_offset + bytes.len();
    assert!(text_end <= pv.len());

    {
        let area = &mut pv[1024..1536];
        area[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
        area[20..24].copy_from_slice(&1u32.to_le_bytes());
        area[24..32].copy_from_slice(&1024u64.to_le_bytes());
        area[32..40].copy_from_slice(&1536u64.to_le_bytes());
        area[40..48].copy_from_slice(&512u64.to_le_bytes());
    }
    pv[text_offset..text_end].copy_from_slice(bytes);

    let text_crc = fs_lvm::crc::lvm_crc32(bytes);
    let area = &mut pv[1024..1536];
    area[48..56].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
    area[56..60].copy_from_slice(&text_crc.to_le_bytes());
    let area_crc = fs_lvm::crc::lvm_crc32(&area[4..512]);
    area[0..4].copy_from_slice(&area_crc.to_le_bytes());
}
