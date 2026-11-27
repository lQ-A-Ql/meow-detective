use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

use app_services::datasource_service::{
    self, ImageCollectionPlan, ImageFilesystemSource, LvmDiscoverySource,
};
use app_services::import_pipeline::{execute_import_job, ImportJobOptions};
use domain::{DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};

fn native_pvs(primary: &Path, supplementary: &Path) {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/lvm-native/shared-ext4");
    for (path, index) in [(primary, 1), (supplementary, 0)] {
        std::io::copy(
            &mut flate2::read::GzDecoder::new(
                File::open(fixtures.join(format!("pv-{index}.raw.gz"))).unwrap(),
            ),
            &mut File::create(path).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn source_local_expansion_does_not_scan_a_neighboring_pv_image() {
    let root = tempfile::tempdir().unwrap();
    let primary = root.path().join("primary.raw");
    let neighbor = root.path().join("neighbor.raw");
    native_pvs(&primary, &neighbor);
    let mut reader = evidence_core::RawImageReader::open(&primary).unwrap();
    let mut probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    datasource_service::expand_lvm_pool_candidates(&mut probe, &primary, &DataSourceKind::Raw);
    assert!(probe
        .candidates
        .iter()
        .all(|candidate| candidate.source != ImageFilesystemSource::LvmLogicalVolume));
    assert!(probe
        .warnings
        .iter()
        .any(|warning| warning.contains("skipping incomplete")));
    assert!(probe
        .warnings
        .iter()
        .all(|warning| !warning.contains(neighbor.to_string_lossy().as_ref())));
}

#[test]
fn ordinary_import_does_not_consume_a_case_registered_supplementary_pv() {
    let root = tempfile::tempdir().unwrap();
    let primary = root.path().join("primary.raw");
    let supplementary = root.path().join("other-source.raw");
    native_pvs(&primary, &supplementary);
    let cancel = Arc::new(AtomicBool::new(false));
    let sources = vec![
        LvmDiscoverySource::new(&primary, DataSourceKind::Raw),
        LvmDiscoverySource::new(&supplementary, DataSourceKind::Raw),
    ];
    let plan = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    assert_eq!(
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                plan.member_probe(index, &source.source_path, &source.source_kind)
                    .unwrap()
                    .candidates
                    .len()
            })
            .sum::<usize>(),
        1
    );

    let case =
        app_services::case_service::create_case(root.path(), "LVM source isolation", None).unwrap();
    let connection = case.connection().unwrap();
    let unrelated = datasource_service::attach_data_source(
        &connection,
        &case.meta.id,
        "Unrelated PV",
        &supplementary,
        DataSourceKind::Raw,
        DataSourcePlatform::Linux,
    )
    .unwrap();
    let job = JobRepo::new(&connection)
        .create(&case.meta.id.0, "Source isolation")
        .unwrap();
    let config = app_services::import_precheck::prepare_import_source_config_from_path(
        &primary.to_string_lossy(),
        DataSourcePlatform::Linux,
    )
    .unwrap();
    assert!(config.collection_plan.is_none());
    assert!(execute_import_job(
        &connection,
        &case.meta.id,
        &case.case_root,
        config,
        &job,
        ImportJobOptions {
            projection_policy: Default::default(),
            event_sink: None,
            cancel_token: &cancel,
            max_import_workers: Some(1),
            max_analysis_workers: Some(1),
            analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
            defer_linux_artifacts: false,
        },
    )
    .is_err());
    let source = DataSourceRepo::new(&connection)
        .find_by_case(&case.meta.id)
        .unwrap()
        .into_iter()
        .find(|source| source.source_path == primary)
        .unwrap();
    assert_ne!(source.id, unrelated.id);
    let source_connection =
        app_services::source_db::open_source_db(&case.case_root, &source.id).unwrap();
    let partitions =
        app_services::partition_query_service::PartitionQueryRepo::new(&source_connection)
            .find_by_data_source(&source.id.0)
            .unwrap();
    assert!(!partitions.is_empty());
    assert!(partitions.iter().all(
        |partition| partition.lvm_lv_uuid.is_none() && partition.lvm_pv_sources_json.is_none()
    ));
}
