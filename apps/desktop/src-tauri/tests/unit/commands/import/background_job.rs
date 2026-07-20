use std::collections::HashSet;
use std::fmt::Write;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Once};
use std::time::{Duration, Instant, SystemTime};

use app_services::ceph_reconstruction::{
    discover_rbd_images_from_source_dbs, materialize_rbd_sources_for_cluster, open_rbd_head_image,
    verify_derived_source_catalog, RadosReplicaSource, SourceDbRadosObjectProvider,
};
use app_services::cluster_service::{plan_linux_cluster_import, LinuxClusterImportPlan};
use app_services::datasource_service::{self, ImageFilesystemKind, PartitionStatus};
use app_services::import_analysis::ImportAnalysisMode;
use app_services::source_db::{self, GlobalFileId};
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, FileEntryId};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    audit_repo::AuditRepo,
    case_repo::CaseRepo,
    ceph_bluestore_semantic_repo::{
        latest_state_set_sha256, BLUESTORE_SEMANTIC_DECODE_PROFILE,
        BLUESTORE_SEMANTIC_SCHEMA_VERSION,
    },
    ceph_rbd_lineage_repo::CephRbdLineageRepo,
    ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRepo,
    datasource_cluster_repo::DataSourceClusterRepo,
    datasource_repo::DataSourceRepo,
    file_repo::FileRepo,
    job_repo::JobRepo,
    processing_phase_repo::{DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseState},
    timeline_repo::TimelineRepo,
};
use tempfile::TempDir;
use transport::dto::ViewerRangeRequestDto;

use super::{
    complete_browseable_cluster_job, continue_cluster_rbd_processing,
    run_background_linux_cluster_import_until_browseable, BackgroundLinuxClusterImportJob,
};

const PVE_CLUSTER_ROOT_ENV: &str = "FORENSICS_PVE_CLUSTER_ROOT";
const PVE_CASE_OUTPUT_ROOT_ENV: &str = "FORENSICS_PVE_CASE_OUTPUT_ROOT";
const PVE_RBD_CASE_ROOT_ENV: &str = "FORENSICS_PVE_RBD_CASE_ROOT";
const PVE_RBD_DEEP_PARENT_HASH_ENV: &str = "FORENSICS_PVE_RBD_DEEP_PARENT_HASH";
const PVE_RBD_COLD_ARTIFACT_REPLAY_ENV: &str = "FORENSICS_PVE_RBD_COLD_ARTIFACT_REPLAY";
const PVE_RBD_CATALOG_REBUILD_ENV: &str = "FORENSICS_PVE_RBD_CATALOG_REBUILD";
const PVE_MEMBER_COUNT: usize = 6;
const PVE_HOST_COUNT: usize = 3;
const PVE_CLUSTER_FSID: &str = "3f28d8bb-e754-475b-b471-b9c97161bbf7";
const PVE_RBD_IMAGE_ID: &str = "16ecc87af5c9";
const PVE_RBD_IMAGE_NAME: &str = "vm-100-disk-0";
const PVE_RBD_POOL_ID: i64 = 2;
const PVE_RBD_REPLICA_COUNT: usize = 3;
const PVE_RBD_RECORD_COUNT: u64 = 114_260;
const PVE_RBD_DIRECTORY_COUNT: u64 = 15_749;
const PVE_RBD_FILE_COUNT: u64 = 98_511;
const PVE_RBD_TOTAL_FILE_SIZE: u64 = 5_547_104_746;
const PVE_MEMBER_RELATIVE_PATHS: [&str; PVE_MEMBER_COUNT] = [
    "server01/server01-disk01.E01",
    "server01/server01-disk02.E01",
    "server02/server02-disk01.E01",
    "server02/server02-disk02.E01",
    "server03/server03-disk01.E01",
    "server03/server03-disk02.E01",
];
const PVE_OS_FILES: &[&str] = &[
    "/etc/passwd",
    "/etc/os-release",
    "/etc/hostname",
    "/var/lib/pve-cluster/config.db",
];

fn run_background_linux_cluster_import_job(
    job: BackgroundLinuxClusterImportJob,
    app: Option<&tauri::AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), transport::CommandError> {
    let processing =
        run_background_linux_cluster_import_until_browseable(job, app, cancel_token.clone())?;
    if let Some(outcome) = processing {
        complete_browseable_cluster_job(&outcome, app)?;
        continue_cluster_rbd_processing(&outcome.processing, &cancel_token)?;
    }
    Ok(())
}

struct BluestoreSourceSummary {
    osd_uuid: String,
    osd_id: Option<u32>,
    ceph_fsid: Option<String>,
    bluefs_uuid: String,
}

struct BluestoreOracle {
    osd_id: u32,
    osd_uuid: &'static str,
    selected_epoch: i64,
    ceph_fsid: &'static str,
    bluefs_uuid: &'static str,
    crc32c: u32,
    extent_offset: u64,
    final_sequence: u64,
    file_count: usize,
    manifest_path: &'static str,
    wal_path: &'static str,
    rocksdb_identity: &'static str,
    manifest_file_number: u64,
    manifest_file_size: u64,
    rocksdb_live_sst_count: usize,
    rocksdb_next_file_number: u64,
    rocksdb_last_sequence: u64,
    rocksdb_log_number: u64,
    rocksdb_sst_data_block_count: u64,
    rocksdb_sst_entry_count: u64,
    rocksdb_wal_number: u64,
    rocksdb_wal_file_size: u64,
    rocksdb_wal_record_count: u32,
    rocksdb_wal_empty_batch_count: u32,
    rocksdb_wal_mutation_count: u64,
    rocksdb_wal_payload_bytes: u64,
    rocksdb_wal_first_sequence: u64,
    rocksdb_wal_last_sequence: u64,
    rocksdb_latest_state_sha256: &'static str,
    semantic: BluestoreSemanticOracle,
    representative_sst: Option<RepresentativeSstOracle>,
}

struct BluestoreSemanticOracle {
    semantic_sha256: &'static str,
    collection_count: u64,
    object_count: u64,
    blob_count: u64,
    onode_shard_count: u64,
    logical_extent_count: u64,
    physical_extent_count: u64,
    checksum_chunk_count: u64,
    shared_blob_count: u64,
    shared_ref_extent_count: u64,
}

struct RepresentativeSstOracle {
    file_number: u64,
    data_block_count: u64,
    entry_count: u64,
    deletion_count: u64,
    raw_key_size: u64,
    raw_value_size: u64,
    data_size: u64,
    index_size: u64,
    filter_size: u64,
}

struct BluefsInventoryRow {
    inventory_id: String,
    bluefs_uuid: String,
    osd_uuid: String,
    sequence: u64,
    block_size: u32,
    crc32c: u32,
    shared_bdev: Option<u32>,
    dedicated_db: Option<bool>,
    dedicated_wal: Option<bool>,
}

struct RocksDbManifestRow {
    active_manifest_path: String,
    identity_uuid: String,
    manifest_file_number: u64,
    manifest_file_size: u64,
    logical_edit_count: u32,
    comparator_name: String,
    last_sequence: u64,
    next_file_number: u64,
    log_number: u64,
    prev_log_number: u64,
    max_column_family_id: u32,
    min_log_number_to_keep: Option<u64>,
}

#[derive(Debug)]
struct RocksDbLatestStateRow {
    column_family_id: u32,
    column_family_name: String,
    point_mutation_count: u64,
    sst_point_mutation_count: u64,
    wal_point_mutation_count: u64,
    range_mutation_count: u64,
    sst_range_mutation_count: u64,
    wal_range_mutation_count: u64,
    latest_value_count: u64,
    deleted_key_count: u64,
    delete_decision_count: u64,
    single_delete_decision_count: u64,
    range_delete_decision_count: u64,
    merge_resolved_count: u64,
    merge_operand_count: u64,
    range_hidden_version_count: u64,
    smallest_sequence: Option<u64>,
    largest_sequence: Option<u64>,
    sharding_sha256: String,
    point_sha256: String,
    range_sha256: String,
    latest_state_sha256: String,
}

#[derive(Debug, PartialEq, Eq)]
struct SourceDbReadOnlySnapshot {
    source_id: String,
    length: u64,
    modified_at: SystemTime,
    boundary_sha256: String,
    full_sha256: Option<String>,
}

#[test]
#[ignore = "requires the private six-member PVE E01 cluster fixture"]
fn real_pve_cluster_import_attempts_every_member_and_isolates_source_databases() {
    init_test_tracing();
    let fixture_root = required_fixture_root();
    let plan = plan_linux_cluster_import(&fixture_root, Some("pve-cluster".to_string()))
        .expect("plan PVE cluster import");
    assert_plan(&fixture_root, &plan);

    let (_temp, case_root) = pve_case_root("pve-cluster-case");
    let case_id = CaseId("pve-cluster-import-regression".to_string());
    let case_conn = create_case_database(&case_root, &case_id);
    let job_id = JobRepo::new(&case_conn)
        .create(&case_id.0, "linux-cluster-import")
        .expect("create cluster import job");
    drop(case_conn);

    let total_started = Instant::now();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let scheduler_before = app_services::import_scheduler::global_import_admission().snapshot();
    let browseable_started = Instant::now();
    let processing = run_background_linux_cluster_import_until_browseable(
        BackgroundLinuxClusterImportJob {
            db_path: case_root.join("app.db"),
            case_id: case_id.clone(),
            case_root: case_root.clone(),
            plan: plan.clone(),
            job_id: job_id.clone(),
            max_import_workers: None,
            max_analysis_workers: None,
            analysis_mode: ImportAnalysisMode::MetadataOnly,
        },
        None,
        cancel_token.clone(),
    );
    let browseable_elapsed = browseable_started.elapsed();
    let scheduler_after = app_services::import_scheduler::global_import_admission().snapshot();
    eprintln!(
        "PVE scheduler: activeBefore={} activeAfter={} peakSources={} peakCpuWeight={} peakMemoryReservationMb={} rssMb={} peakRssMb={}",
        scheduler_before.active_sources,
        scheduler_after.active_sources,
        scheduler_after.peak_active_sources,
        scheduler_after.peak_cpu_in_use,
        scheduler_after.peak_memory_in_use_mb,
        app_services::import_analysis::current_rss_mb(),
        app_services::import_analysis::peak_rss_mb()
    );
    let parent_snapshots = if processing.is_ok() {
        let browseable_conn =
            persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
                .expect("open browseable case database");
        capture_rbd_parent_source_snapshots(&browseable_conn, &case_root, &case_id)
    } else {
        Vec::new()
    };
    let result = processing.and_then(|processing| {
        let post_processing_started = Instant::now();
        let result = match processing {
            Some(outcome) => complete_browseable_cluster_job(&outcome, None)
                .and_then(|()| continue_cluster_rbd_processing(&outcome.processing, &cancel_token)),
            None => Ok(()),
        };
        eprintln!(
            "PVE cluster timing: browseableMs={} postProcessingMs={} totalMs={}",
            browseable_elapsed.as_millis(),
            post_processing_started.elapsed().as_millis(),
            total_started.elapsed().as_millis()
        );
        result
    });
    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("reopen case database");
    let cluster = DataSourceClusterRepo::new(&case_conn)
        .find_by_id(&plan.cluster_id)
        .expect("query cluster")
        .expect("cluster record");
    eprintln!(
        "PVE cluster outcome: runner={result:?}, state={}, ready={}, failed={}, error={:?}",
        cluster.import_state, cluster.ready_count, cluster.failed_count, cluster.last_error
    );
    let sources = DataSourceRepo::new(&case_conn)
        .find_by_case(&case_id)
        .expect("query diagnostic data sources");
    for source in &sources {
        let storage = DataSourceRepo::new(&case_conn)
            .find_storage(&source.id)
            .expect("query diagnostic source storage")
            .expect("diagnostic source storage");
        let source_db_path = source_db::source_db_path(&case_root, &source.id);
        let file_count = if source_db_path.exists() {
            persistence_sqlite::open_existing_source(&source_db_path)
                .ok()
                .and_then(|conn| FileRepo::new(&conn).count_by_data_source(&source.id).ok())
        } else {
            None
        };
        eprintln!(
            "PVE member outcome: name={} state={} files={file_count:?} error={:?}",
            source.name, storage.import_state, storage.last_error
        );
    }
    assert_control_database_is_tree_free(&case_conn);
    assert_manifest(&case_root, &plan);
    assert_member_storage_and_content(&case_conn, &case_root, &case_id, &plan, &cluster);
    assert_derived_rbd_sources(&case_conn, &case_root, &case_id, &plan.cluster_id);
    assert_derived_rbd_automatic_processing(&case_conn, &case_root, &case_id, &plan.cluster_id);
    assert_parent_source_snapshots_unchanged(&case_conn, &case_root, &case_id, &parent_snapshots);
    assert_job_outcome(&case_conn, &job_id, &cluster);
}

fn pve_case_root(case_name: &str) -> (Option<TempDir>, PathBuf) {
    if let Some(output_root) = std::env::var_os(PVE_CASE_OUTPUT_ROOT_ENV) {
        let output_root = PathBuf::from(output_root);
        assert!(
            output_root.is_absolute(),
            "{PVE_CASE_OUTPUT_ROOT_ENV} must be an absolute path"
        );
        std::fs::create_dir_all(&output_root).expect("create retained PVE output root");
        let case_root = output_root.join(case_name);
        assert!(
            !case_root.exists(),
            "retained PVE case path already exists: {}",
            case_root.display()
        );
        std::fs::create_dir_all(&case_root).expect("create retained PVE case directory");
        eprintln!("PVE retained case root: {}", case_root.display());
        return (None, case_root);
    }

    let temp = TempDir::new().expect("create temporary case root");
    let case_root = temp.path().join(case_name);
    std::fs::create_dir_all(&case_root).expect("create temporary PVE case directory");
    (Some(temp), case_root)
}

#[test]
#[ignore = "requires the private PVE E01 cluster fixture"]
fn real_pve_bluestore_member_persists_semantic_snapshot() {
    init_test_tracing();
    let fixture_root = required_fixture_root();
    let mut plan = plan_linux_cluster_import(&fixture_root, Some("pve-cluster".to_string()))
        .expect("plan PVE cluster import");
    plan.members.retain(|member| {
        member
            .source_name
            .eq_ignore_ascii_case("server01-disk02.E01")
    });
    assert_eq!(plan.members.len(), 1, "server01 BlueStore member");
    plan.members[0].member_index = 0;

    let (_temp, case_root) = pve_case_root("pve-bluestore-case");
    let case_id = CaseId("pve-bluestore-import-regression".to_string());
    let case_conn = create_case_database(&case_root, &case_id);
    let job_id = JobRepo::new(&case_conn)
        .create(&case_id.0, "pve-bluestore-import")
        .expect("create import job");
    drop(case_conn);

    run_background_linux_cluster_import_job(
        BackgroundLinuxClusterImportJob {
            db_path: case_root.join("app.db"),
            case_id: case_id.clone(),
            case_root: case_root.clone(),
            plan,
            job_id,
            max_import_workers: Some(1),
            max_analysis_workers: Some(1),
            analysis_mode: ImportAnalysisMode::MetadataOnly,
        },
        None,
        Arc::new(AtomicBool::new(false)),
    )
    .expect("import BlueStore member");

    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("reopen case database");
    let source = DataSourceRepo::new(&case_conn)
        .find_by_case(&case_id)
        .expect("query data sources")
        .into_iter()
        .next()
        .expect("imported BlueStore source");
    assert_bluestore_source(&case_conn, &case_root, &source);
}

#[test]
#[ignore = "requires the private three-OSD PVE RBD fixture"]
fn real_pve_rbd_head_image_byte_oracle() {
    init_test_tracing();
    let (_temp, case_root) = prepare_rbd_oracle_case();
    let (replicas, osd_ids) = load_rbd_replicas(&case_root);
    assert_eq!(osd_ids, vec![0, 1, 2]);

    let descriptors =
        discover_rbd_images_from_source_dbs(&replicas).expect("discover replicated RBD images");
    let descriptor = descriptors
        .into_iter()
        .find(|descriptor| descriptor.metadata.id == PVE_RBD_IMAGE_ID)
        .expect("vm-100 RBD image");
    assert_eq!(descriptor.metadata.name, PVE_RBD_IMAGE_NAME);
    assert_eq!(descriptor.metadata.data_pool_id, PVE_RBD_POOL_ID);
    assert_eq!(descriptor.metadata.image_size, 60 * 1024 * 1024 * 1024);
    assert_eq!(descriptor.metadata.order, 22);
    assert_eq!(descriptor.metadata.features, 0x3d);
    assert_eq!(descriptor.context.operation_features, 0);
    assert!(!descriptor.context.has_parent);

    let provider = SourceDbRadosObjectProvider::new(
        replicas,
        descriptor.metadata.data_pool_id,
        Vec::new(),
        PVE_RBD_REPLICA_COUNT,
    )
    .expect("open closed RBD replica provider");
    let mut reader =
        open_rbd_head_image(&descriptor, Box::new(provider)).expect("open RBD head image");
    let probe =
        datasource_service::detect_image_filesystem(&mut reader).expect("probe RBD filesystem");
    assert!(!probe.partitions.is_empty(), "RBD partition table");
    assert!(
        !probe.candidates.is_empty(),
        "RBD image must expose a supported filesystem candidate"
    );

    let partition = probe
        .partitions
        .iter()
        .find(|partition| {
            partition.offset > 0
                && matches!(
                    partition.status,
                    PartitionStatus::Supported | PartitionStatus::Expanded
                )
        })
        .expect("supported RBD partition");
    let candidate = probe
        .candidates
        .iter()
        .find(|candidate| candidate.offset == partition.offset)
        .or_else(|| probe.candidates.first())
        .expect("RBD filesystem candidate");
    let object_size = 1u64 << descriptor.metadata.order;
    let filesystem_offset = filesystem_oracle_offset(candidate.kind, candidate.offset);
    let probes = [
        ("image-head", 0),
        ("object-boundary", object_size - 2048),
        ("partition-head", partition.offset),
        ("filesystem-superblock", filesystem_offset),
        ("image-tail", descriptor.metadata.image_size - 4096),
    ];
    let mut hashes = Vec::with_capacity(probes.len());
    for (label, offset) in probes {
        let digest = hash_reader_range(&mut reader, offset, 4096);
        eprintln!("RBD_BYTE_ORACLE label={label} offset={offset} sha256={digest}");
        hashes.push((label, digest));
    }
    assert_rbd_oracle_hashes(&hashes);
    eprintln!(
        "RBD_LAYOUT_ORACLE partitions={:?} candidate_kind={:?} candidate_offset={} warnings={:?}",
        probe
            .partitions
            .iter()
            .map(|partition| (
                partition.index,
                partition.kind_label.as_str(),
                partition.offset,
                partition.length,
                partition.status
            ))
            .collect::<Vec<_>>(),
        candidate.kind,
        candidate.offset,
        probe.warnings
    );
}

#[test]
#[ignore = "requires a retained PVE case with imported OSD source databases"]
fn real_pve_rbd_materializes_vm_tree_from_retained_cluster() {
    init_test_tracing();
    let case_root = std::env::var_os(PVE_RBD_CASE_ROOT_ENV)
        .map(PathBuf::from)
        .expect("FORENSICS_PVE_RBD_CASE_ROOT must point to a retained PVE case root");
    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("open retained PVE case database");
    persistence_sqlite::runner::run_all(&case_conn).expect("migrate retained PVE case database");
    let cluster_id = case_conn
        .query_row(
            "SELECT id FROM data_source_clusters ORDER BY created_at, id LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("query retained PVE cluster id");
    let cluster = DataSourceClusterRepo::new(&case_conn)
        .find_by_id(&cluster_id)
        .expect("query retained PVE cluster")
        .expect("retained PVE cluster");
    let require_ready = std::env::var_os("FORENSICS_PVE_RBD_REQUIRE_READY").is_some();
    let catalog_rebuild = std::env::var_os(PVE_RBD_CATALOG_REBUILD_ENV).is_some();
    assert!(
        !(require_ready && catalog_rebuild),
        "retained ready replay and Catalog rebuild are mutually exclusive"
    );
    if require_ready {
        let ready_derived = DataSourceRepo::new(&case_conn)
            .find_by_case(&cluster.case_id)
            .expect("query retained derived sources")
            .into_iter()
            .filter(|source| source.kind == domain::DataSourceKind::CephRbd)
            .filter(|source| {
                DataSourceRepo::new(&case_conn)
                    .find_storage(&source.id)
                    .expect("query retained derived storage")
                    .is_some_and(|storage| storage.import_state == "ready")
            })
            .count();
        assert_eq!(
            ready_derived, 1,
            "retained performance mode requires an already materialized ready RBD source"
        );
    }
    DataSourceClusterRepo::new(&case_conn)
        .update_state(&cluster_id, "ready", cluster.member_count, 0, None)
        .expect("mark retained PVE cluster ready for RBD materialization");
    migrate_retained_source_databases(
        &case_conn,
        &case_root,
        &cluster.case_id,
        if catalog_rebuild { 0 } else { 1 },
    );
    let parent_snapshots =
        capture_rbd_parent_source_snapshots(&case_conn, &case_root, &cluster.case_id);

    let started = Instant::now();
    let materialized =
        materialize_rbd_sources_for_cluster(&case_conn, &case_root, &cluster.case_id, &cluster_id)
            .expect("materialize retained PVE RBD sources");
    eprintln!(
        "PVE_RBD_MATERIALIZE elapsedMs={} sources={}",
        started.elapsed().as_millis(),
        materialized.len()
    );
    if require_ready {
        assert!(
            started.elapsed() <= Duration::from_secs(5),
            "ready derived-source reuse exceeded 5 seconds and may have regressed to full Catalog work"
        );
    }

    assert_eq!(materialized.len(), 1);
    assert_parent_source_snapshots_unchanged(
        &case_conn,
        &case_root,
        &cluster.case_id,
        &parent_snapshots,
    );
    assert_derived_rbd_sources(&case_conn, &case_root, &cluster.case_id, &cluster_id);
    if catalog_rebuild {
        return;
    }
    let cold_artifact_replay = std::env::var_os(PVE_RBD_COLD_ARTIFACT_REPLAY_ENV).is_some();
    if cold_artifact_replay {
        reset_derived_artifact_phase(
            &case_conn,
            &case_root,
            &materialized[0].data_source.id,
            true,
        );
    }
    for source in &materialized {
        app_services::ceph_reconstruction::finalize_rbd_source_processing(
            &case_conn,
            &case_root,
            &cluster.case_id,
            &source.data_source.id,
        )
        .expect("finalize retained PVE RBD source processing");
    }
    if cold_artifact_replay {
        assert_cold_artifact_phase_metrics(
            &case_conn,
            &materialized[0].data_source.id,
            PVE_RBD_FILE_COUNT,
        );
        reset_derived_artifact_phase(
            &case_conn,
            &case_root,
            &materialized[0].data_source.id,
            false,
        );
        app_services::ceph_reconstruction::finalize_rbd_source_processing(
            &case_conn,
            &case_root,
            &cluster.case_id,
            &materialized[0].data_source.id,
        )
        .expect("run idempotent retained PVE artifact replay");
        assert_idempotent_artifact_phase_metrics(&case_conn, &materialized[0].data_source.id);
    }
    assert_derived_rbd_automatic_processing(&case_conn, &case_root, &cluster.case_id, &cluster_id);
    assert_parent_source_snapshots_unchanged(
        &case_conn,
        &case_root,
        &cluster.case_id,
        &parent_snapshots,
    );
}

fn migrate_retained_source_databases(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    expected_derived_sources: usize,
) {
    let sources = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("query retained source databases before migration");
    let derived_source_count = sources
        .iter()
        .filter(|source| source.kind == domain::DataSourceKind::CephRbd)
        .count();
    assert_eq!(
        derived_source_count, expected_derived_sources,
        "retained RBD replay has an unexpected derived-source baseline"
    );
    let started = Instant::now();
    for source in &sources {
        let source_conn = source_db::open_registered_source_db(case_conn, case_root, &source.id)
            .unwrap_or_else(|error| {
                panic!(
                    "migrate retained source database '{}': {error}",
                    source.id.0
                )
            });
        source_db::checkpoint_source_db(&source_conn).unwrap_or_else(|error| {
            panic!(
                "checkpoint retained source database '{}': {error}",
                source.id.0
            )
        });
    }
    eprintln!(
        "PVE_RBD_SOURCE_MIGRATION elapsedMs={} sources={}",
        started.elapsed().as_millis(),
        sources.len()
    );
}

fn reset_derived_artifact_phase(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    clear_checkpoints: bool,
) {
    if clear_checkpoints {
        let source_conn =
            source_db::open_registered_source_db(case_conn, case_root, data_source_id)
                .expect("open derived source for cold artifact replay");
        source_conn
            .execute(
                "DELETE FROM source_meta
                 WHERE key LIKE 'analysis_candidate_scan:%'",
                [],
            )
            .expect("clear derived artifact checkpoints");
    }
    let changed = case_conn
        .execute(
            "UPDATE data_source_processing_phases
             SET state = 'failed',
                 stats_json = '{}',
                 last_error = 'explicit real-sample artifact replay',
                 completed_at = datetime('now'),
                 heartbeat_at = datetime('now'),
                 lease_expires_at = NULL,
                 updated_at = datetime('now')
             WHERE data_source_id = ?1
               AND phase = 'artifacts'",
            [&data_source_id.0],
        )
        .expect("reset derived artifact processing phase");
    assert_eq!(
        changed, 1,
        "derived artifact phase must exist before replay"
    );
}

fn assert_cold_artifact_phase_metrics(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    max_file_count: u64,
) {
    let stats = artifact_phase_stats(case_conn, data_source_id);
    let scanned = required_u64(&stats, "scannedCount");
    let source_reads = required_u64(&stats, "sourceReadCount");
    let source_read_elapsed_ms = required_u64(&stats, "sourceReadElapsedMs");
    let processing_elapsed_ms = required_u64(&stats, "processingElapsedMs");
    assert!(scanned > 0 && scanned <= max_file_count);
    assert_eq!(
        source_reads, scanned,
        "cold artifact replay must read every non-checkpointed candidate"
    );
    assert!(source_read_elapsed_ms > 0);
    assert!(processing_elapsed_ms >= source_read_elapsed_ms);
    assert!(required_u64(&stats, "sourceReadAvgMicros") > 0);
    assert_eq!(
        required_u64(&stats, "radosReadPlanSessionInitializations"),
        PVE_RBD_REPLICA_COUNT as u64,
        "each immutable BlueStore parent source must initialize one read-plan session"
    );
    assert!(
        required_u64(&stats, "radosPlanCacheMisses")
            >= required_u64(&stats, "radosReadPlanSessionInitializations"),
        "object-plan misses must not reinitialize source-level bindings"
    );
    assert!(stats["rssMb"].is_number());
    assert!(stats["peakRssMb"].is_number());
    eprintln!("PVE_RBD_ARTIFACT_COLD stats={stats}");
}

fn assert_idempotent_artifact_phase_metrics(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
) {
    let stats = artifact_phase_stats(case_conn, data_source_id);
    let scanned = required_u64(&stats, "scannedCount");
    assert_eq!(
        required_u64(&stats, "sourceReadCount"),
        0,
        "checkpoint replay must not read derived evidence bytes"
    );
    assert_eq!(
        required_u64(&stats, "checkpointHitCount"),
        scanned,
        "every idempotent replay candidate must resolve from a checkpoint"
    );
    eprintln!("PVE_RBD_ARTIFACT_IDEMPOTENT stats={stats}");
}

fn artifact_phase_stats(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
) -> serde_json::Value {
    let phase = DataSourceProcessingPhaseRepo::new(case_conn)
        .find(data_source_id, ProcessingPhase::Artifacts)
        .expect("query derived artifact phase")
        .expect("derived artifact phase");
    assert_eq!(phase.state, ProcessingPhaseState::Ready);
    serde_json::from_str(&phase.stats_json).expect("parse derived artifact phase stats")
}

fn required_u64(value: &serde_json::Value, field: &str) -> u64 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("artifact phase stats field '{field}' is missing: {value}"))
}

#[test]
#[ignore = "requires a retained PVE case with imported cluster data"]
fn real_pve_cluster_asserts_retained_source_isolation_and_derived_rbd() {
    init_test_tracing();
    let fixture_root = required_fixture_root();
    let mut plan = plan_linux_cluster_import(&fixture_root, Some("pve-cluster".to_string()))
        .expect("plan retained PVE cluster");
    let case_root = std::env::var_os(PVE_RBD_CASE_ROOT_ENV)
        .map(PathBuf::from)
        .expect("FORENSICS_PVE_RBD_CASE_ROOT must point to a retained PVE case root");
    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("open retained PVE case database");
    let cluster_id = case_conn
        .query_row(
            "SELECT id FROM data_source_clusters ORDER BY created_at, id LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("query retained PVE cluster id");
    let cluster = DataSourceClusterRepo::new(&case_conn)
        .find_by_id(&cluster_id)
        .expect("query retained PVE cluster")
        .expect("retained PVE cluster");
    plan.cluster_id = cluster_id.clone();
    plan.manifest_rel_path = format!("clusters/{cluster_id}/cluster-manifest.json");

    assert_control_database_is_tree_free(&case_conn);
    assert_manifest(&case_root, &plan);
    assert_member_storage_and_content(&case_conn, &case_root, &cluster.case_id, &plan, &cluster);
    assert_derived_rbd_sources(&case_conn, &case_root, &cluster.case_id, &cluster_id);
}

fn prepare_rbd_oracle_case() -> (Option<TempDir>, PathBuf) {
    if let Some(case_root) = std::env::var_os(PVE_RBD_CASE_ROOT_ENV).map(PathBuf::from) {
        assert!(
            case_root.join("app.db").is_file(),
            "{PVE_RBD_CASE_ROOT_ENV} must point to a retained case root"
        );
        return (None, case_root);
    }

    let fixture_root = required_fixture_root();
    let mut plan = plan_linux_cluster_import(&fixture_root, Some("pve-rbd-oracle".to_string()))
        .expect("plan PVE RBD import");
    plan.members
        .retain(|member| member.source_name.ends_with("-disk02.E01"));
    assert_eq!(plan.members.len(), PVE_RBD_REPLICA_COUNT);
    for (index, member) in plan.members.iter_mut().enumerate() {
        member.member_index = index as u32;
    }

    let (temp, case_root) = pve_case_root("pve-rbd-byte-oracle-case");
    let case_id = CaseId("pve-rbd-byte-oracle-regression".to_string());
    let case_conn = create_case_database(&case_root, &case_id);
    let job_id = JobRepo::new(&case_conn)
        .create(&case_id.0, "pve-rbd-byte-oracle-import")
        .expect("create RBD oracle import job");
    drop(case_conn);

    run_background_linux_cluster_import_job(
        BackgroundLinuxClusterImportJob {
            db_path: case_root.join("app.db"),
            case_id,
            case_root: case_root.clone(),
            plan,
            job_id,
            max_import_workers: Some(1),
            max_analysis_workers: Some(1),
            analysis_mode: ImportAnalysisMode::MetadataOnly,
        },
        None,
        Arc::new(AtomicBool::new(false)),
    )
    .expect("import three BlueStore replicas");
    (temp, case_root)
}

fn load_rbd_replicas(case_root: &Path) -> (Vec<RadosReplicaSource>, Vec<u32>) {
    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("open RBD oracle case database");
    let case_id = case_conn
        .query_row(
            "SELECT id FROM cases ORDER BY created_at LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .map(CaseId)
        .expect("RBD oracle case id");
    let sources = DataSourceRepo::new(&case_conn)
        .find_by_case(&case_id)
        .expect("query RBD replica sources");
    assert_eq!(sources.len(), PVE_RBD_REPLICA_COUNT);

    let mut replicas = sources
        .into_iter()
        .map(|source| {
            let source_db_path = source_db::source_db_path(case_root, &source.id);
            let source_conn = persistence_sqlite::open_existing_source(&source_db_path)
                .expect("open RBD replica source database");
            let (inventory_id, osd_id, ceph_fsid) = source_conn
                .query_row(
                    "SELECT id, whoami, ceph_fsid
                     FROM ceph_osd_inventory
                     WHERE data_source_id = ?1",
                    [&source.id.0],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, u32>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .expect("query RBD replica inventory");
            assert_eq!(ceph_fsid, PVE_CLUSTER_FSID);
            let replica = RadosReplicaSource::new(source.id, inventory_id, source_db_path)
                .expect("bind RBD replica source");
            (osd_id, replica)
        })
        .collect::<Vec<_>>();
    replicas.sort_unstable_by_key(|(osd_id, _)| *osd_id);
    let osd_ids = replicas.iter().map(|(osd_id, _)| *osd_id).collect();
    let replicas = replicas.into_iter().map(|(_, replica)| replica).collect();
    (replicas, osd_ids)
}

fn filesystem_oracle_offset(kind: ImageFilesystemKind, base: u64) -> u64 {
    match kind {
        ImageFilesystemKind::Ext4 => base + 1024,
        ImageFilesystemKind::Btrfs => base + 64 * 1024,
        ImageFilesystemKind::LvmPool => base + 512,
        ImageFilesystemKind::Ntfs
        | ImageFilesystemKind::Fat
        | ImageFilesystemKind::BitLocker
        | ImageFilesystemKind::Xfs => base,
    }
}

fn hash_reader_range(
    reader: &mut app_services::ceph_reconstruction::RbdEvidenceReader,
    offset: u64,
    length: usize,
) -> String {
    reader
        .seek(SeekFrom::Start(offset))
        .expect("seek RBD oracle range");
    let mut bytes = vec![0u8; length];
    reader
        .read_exact(&mut bytes)
        .expect("read RBD oracle range");
    infrastructure::hashing::sha256_bytes(&bytes)
}

fn assert_rbd_oracle_hashes(hashes: &[(&str, String)]) {
    let expected = [
        (
            "image-head",
            "249a056f59ea7ecc4124856f78970f2622778e8bdc75146f95df3d2fdcd3d330",
        ),
        (
            "object-boundary",
            "546e6c33dbeaa8369263dfdae6312012850f965235301680680f4a2b93065924",
        ),
        (
            "partition-head",
            "7b8aaced722dc78ada3d1d9a15a57ef816703cd5fd8564693ec958a124af426b",
        ),
        (
            "filesystem-superblock",
            "7b8aaced722dc78ada3d1d9a15a57ef816703cd5fd8564693ec958a124af426b",
        ),
        (
            "image-tail",
            "ad7facb2586fc6e966c004d7d1d16b024f5805ff7cb47c7a85dabd8b48892ca7",
        ),
    ];
    assert_eq!(hashes.len(), expected.len());
    for ((label, actual), (expected_label, expected_hash)) in hashes.iter().zip(expected) {
        assert_eq!(*label, expected_label);
        assert_eq!(actual.len(), 64);
        assert_eq!(actual, expected_hash, "RBD byte oracle changed for {label}");
    }
}

fn init_test_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_test_writer()
            .try_init();
    });
}

fn required_fixture_root() -> PathBuf {
    let root = std::env::var_os(PVE_CLUSTER_ROOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {PVE_CLUSTER_ROOT_ENV} before running this ignored test"));
    assert!(root.is_dir(), "PVE fixture root is not a directory");
    root
}

fn assert_plan(fixture_root: &Path, plan: &LinuxClusterImportPlan) {
    assert_eq!(plan.root_path, fixture_root);
    assert_eq!(plan.members.len(), PVE_MEMBER_COUNT);
    for (expected_index, (member, expected_relative_path)) in plan
        .members
        .iter()
        .zip(PVE_MEMBER_RELATIVE_PATHS)
        .enumerate()
    {
        let expected_path = fixture_root.join(expected_relative_path);
        let expected_name = expected_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("PVE member file name");
        assert_eq!(member.member_index, expected_index as u32);
        assert_eq!(member.source_path, expected_path);
        assert_eq!(member.source_name, expected_name);
        assert!(
            member
                .source_path
                .extension()
                .is_some_and(|extension| extension == "E01"),
            "PVE member must use the exact .E01 primary segment name: {}",
            member.source_path.display()
        );
    }
}

fn create_case_database(case_root: &Path, case_id: &CaseId) -> rusqlite::Connection {
    std::fs::create_dir_all(case_root.join("cache"))
        .expect("create case cache directory required by data-source lifecycle");
    let conn = persistence_sqlite::connection::open_or_create(&case_root.join("app.db"))
        .expect("create app database");
    persistence_sqlite::runner::run_all(&conn).expect("run app migrations");
    let now = chrono::Utc::now();
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "PVE Cluster Import Regression".to_string(),
            number: None,
            examiner: Some("real-sample-regression".to_string()),
            notes: Some("Six-member serial PVE import isolation regression".to_string()),
            created_at: now,
            updated_at: now,
        })
        .expect("insert case");
    conn
}

fn assert_job_outcome(
    case_conn: &rusqlite::Connection,
    job_id: &domain::JobId,
    cluster: &persistence_sqlite::repositories::datasource_cluster_repo::DataSourceClusterRecord,
) {
    let job = JobRepo::new(case_conn)
        .list_recent(20)
        .expect("query jobs")
        .into_iter()
        .find(|job| job.id.0 == job_id.0)
        .expect("cluster import job");
    assert_eq!(job.status, "completed");
    assert_eq!(cluster.import_state, "ready");
    assert_eq!(job.failed_count, cluster.failed_count);
    assert_eq!(job.partial, cluster.failed_count > 0);
}

fn assert_control_database_is_tree_free(case_conn: &rusqlite::Connection) {
    assert_eq!(
        FileRepo::new(case_conn)
            .count_all()
            .expect("count app files"),
        0,
        "app.db must remain a control database"
    );
}

fn assert_manifest(case_root: &Path, plan: &LinuxClusterImportPlan) {
    let manifest_path = case_root.join(&plan.manifest_rel_path);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("read cluster manifest"))
            .expect("parse cluster manifest");
    assert_eq!(manifest["clusterId"], plan.cluster_id);
    assert_eq!(manifest["memberCount"], PVE_MEMBER_COUNT);
    assert_eq!(
        manifest["members"]
            .as_array()
            .expect("manifest members")
            .len(),
        PVE_MEMBER_COUNT
    );
}

fn assert_member_storage_and_content(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    plan: &LinuxClusterImportPlan,
    cluster: &persistence_sqlite::repositories::datasource_cluster_repo::DataSourceClusterRecord,
) {
    let sources = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("query cluster data sources");
    let member_sources = sources
        .iter()
        .filter(|source| {
            plan.members
                .iter()
                .any(|member| member.source_path == source.source_path)
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        member_sources.len(),
        PVE_MEMBER_COUNT,
        "every member must be attempted and registered"
    );
    assert_unique_source_storage(case_conn, &sources);

    let mut ready_count = 0;
    let mut metadata_count = 0;
    let mut osd_ids = HashSet::new();
    let mut osd_uuids = HashSet::new();
    let mut bluefs_uuids = HashSet::new();
    let mut cluster_fsids = HashSet::new();
    for member in &plan.members {
        let source = source_for_member(&member_sources, &member.source_path);
        assert_member_metadata(case_conn, source, plan, member.member_index);
        let storage = DataSourceRepo::new(case_conn)
            .find_storage(&source.id)
            .expect("query member storage")
            .expect("member storage");
        match storage.import_state.as_str() {
            "ready" => ready_count += 1,
            "ready_metadata" => metadata_count += 1,
            state => panic!("member {} ended in unexpected state {state}", source.name),
        }
        if storage.import_state == "ready" {
            assert!(
                is_host_disk(&member.source_path),
                "only PVE host disk01 members may be ready: {}",
                member.source_path.display()
            );
            assert_host_source(case_conn, case_root, case_id, source);
        } else {
            assert!(
                !is_host_disk(&member.source_path),
                "PVE host disk01 member failed unexpectedly: {} ({:?})",
                member.source_path.display(),
                storage.last_error
            );
            let inventory = assert_bluestore_source(case_conn, case_root, source);
            osd_uuids.insert(inventory.osd_uuid);
            osd_ids.insert(inventory.osd_id.expect("BlueStore OSD id"));
            cluster_fsids.insert(inventory.ceph_fsid.expect("BlueStore cluster FSID"));
            bluefs_uuids.insert(inventory.bluefs_uuid);
        }
        assert_source_database_health(case_root, source);
    }
    assert_eq!(ready_count, PVE_HOST_COUNT);
    assert_eq!(metadata_count, PVE_MEMBER_COUNT - PVE_HOST_COUNT);
    assert_eq!(osd_ids, HashSet::from([0, 1, 2]));
    assert_eq!(osd_uuids.len(), 3);
    assert_eq!(bluefs_uuids.len(), 3);
    assert_eq!(cluster_fsids, HashSet::from([PVE_CLUSTER_FSID.to_string()]));
    assert_eq!(
        AuditRepo::new(case_conn)
            .count_by_action("datasource.import")
            .expect("count BlueFS import audit entries"),
        3
    );
    assert_eq!(cluster.member_count as usize, PVE_MEMBER_COUNT);
    assert_eq!(cluster.ready_count as usize, PVE_MEMBER_COUNT);
    assert_eq!(cluster.failed_count, 0);
    assert_eq!(cluster.import_state, "ready");
    assert!(cluster.last_error.is_none());
}

fn assert_derived_rbd_sources(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
) {
    let derived_sources = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("query derived data sources")
        .into_iter()
        .filter(|source| source.kind == domain::DataSourceKind::CephRbd)
        .collect::<Vec<_>>();
    assert_eq!(
        derived_sources.len(),
        1,
        "the PVE fixture must materialize its single RBD VM disk"
    );
    let source = &derived_sources[0];
    assert_eq!(
        source.source_path,
        PathBuf::from(format!("ceph-rbd://{cluster_id}/{PVE_RBD_IMAGE_ID}"))
    );
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(&source.id)
        .expect("query derived RBD storage")
        .expect("derived RBD storage");
    assert_eq!(storage.import_state, "ready");
    assert_eq!(storage.platform, "linux");
    assert_eq!(storage.profile.as_deref(), Some("vm_disk"));

    let lineage = CephRbdLineageRepo::new(case_conn)
        .find_by_data_source(&source.id.0)
        .expect("query derived RBD lineage")
        .expect("derived RBD lineage");
    assert_eq!(lineage.lineage.parent_cluster_id, cluster_id);
    assert_eq!(lineage.lineage.image_id, PVE_RBD_IMAGE_ID);
    assert_eq!(lineage.lineage.image_name, PVE_RBD_IMAGE_NAME);
    assert_eq!(
        lineage.lineage.expected_replica_count as usize,
        PVE_RBD_REPLICA_COUNT
    );

    let source_conn = source_db::open_registered_source_db(case_conn, case_root, &source.id)
        .expect("open derived RBD source database");
    let record_count = FileRepo::new(&source_conn)
        .count_by_data_source(&source.id)
        .expect("count derived RBD files");
    assert_eq!(
        record_count, PVE_RBD_RECORD_COUNT,
        "derived RBD record count differs from the retained sample oracle"
    );
    let (directory_count, file_count, total_file_size) = source_conn
        .query_row(
            "SELECT
                SUM(entry_type = 'directory'),
                SUM(entry_type = 'file'),
                COALESCE(SUM(CASE WHEN entry_type = 'file' THEN size ELSE 0 END), 0)
             FROM file_entries
             WHERE data_source_id = ?1",
            [&source.id.0],
            |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, u64>(2)?,
                ))
            },
        )
        .expect("query derived RBD record oracle");
    assert_eq!(directory_count, PVE_RBD_DIRECTORY_COUNT);
    assert_eq!(file_count, PVE_RBD_FILE_COUNT);
    assert_eq!(total_file_size, PVE_RBD_TOTAL_FILE_SIZE);

    let partition_rows = source_conn
        .prepare(
            "SELECT partition_index, filesystem, status
             FROM data_source_partitions
             WHERE data_source_id = ?1
             ORDER BY partition_index",
        )
        .expect("prepare derived partition query")
        .query_map([&source.id.0], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("query derived RBD partitions")
        .collect::<Result<Vec<_>, _>>()
        .expect("read derived RBD partitions");
    assert_eq!(
        partition_rows,
        vec![
            (0, Some("XFS".to_string()), "supported".to_string()),
            (1, Some("XFS".to_string()), "supported".to_string()),
            (2, Some("XFS".to_string()), "supported".to_string()),
        ],
        "derived RBD partition layout differs from the retained sample oracle"
    );
    assert!(
        partition_rows
            .iter()
            .all(|(_, _, status)| status != "unsupported"),
        "derived RBD source must not expose unsupported partition placeholders"
    );

    let entry_id = find_file_by_linux_suffix(&source_conn, &source.id, "/etc/passwd")
        .expect("find stable derived RBD preview file /etc/passwd");
    let global_id = GlobalFileId::new(source.id.clone(), entry_id).encode();
    let handle = app_services::file_service::open_file_handle_for_case(
        case_conn,
        case_root,
        case_id,
        &global_id.0,
    )
    .expect("open derived RBD preview handle");
    let response = app_services::file_service::read_file_range_for_source_case(
        case_conn,
        case_root,
        case_id,
        &ViewerRangeRequestDto {
            handle_id: handle.handle_id,
            offset: 0,
            length: 512,
        },
    )
    .expect("read derived RBD preview range");
    assert!(
        response.raw_bytes.is_some_and(|bytes| !bytes.is_empty()),
        "derived RBD preview must return bytes"
    );
    assert!(
        verify_derived_source_catalog(case_conn, case_root, case_id, &source.id)
            .expect("deep-verify derived RBD Catalog"),
        "derived RBD Catalog manifest does not match its complete persisted file tree"
    );
}

fn assert_derived_rbd_automatic_processing(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
) {
    let source = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("query derived data sources for analysis")
        .into_iter()
        .find(|source| source.kind == domain::DataSourceKind::CephRbd)
        .expect("derived RBD source for Linux analysis");

    let phases = DataSourceProcessingPhaseRepo::new(case_conn)
        .list_for_data_source(&source.id)
        .expect("list automatic derived-source phases");
    assert_eq!(
        phases.iter().map(|phase| phase.phase).collect::<Vec<_>>(),
        ProcessingPhase::ALL,
        "automatic derived-source processing must persist the complete phase graph"
    );
    assert!(
        phases
            .iter()
            .all(|phase| phase.state == ProcessingPhaseState::Ready),
        "automatic derived-source processing is incomplete: {phases:?}"
    );
    let processing = app_services::processing_phase_service::get_data_source_processing_summary(
        case_conn, &source.id,
    )
    .expect("query derived-source processing summary")
    .expect("derived-source processing summary");
    assert_eq!(processing.state, "ready");
    assert_eq!(processing.total_count, ProcessingPhase::ALL.len() as u32);
    assert_eq!(processing.ready_count, ProcessingPhase::ALL.len() as u32);
    assert_eq!(processing.failed_count, 0);
    assert_eq!(processing.deferred_count, 0);
    assert_eq!(
        processing
            .phases
            .iter()
            .map(|phase| phase.phase.as_str())
            .collect::<Vec<_>>(),
        ProcessingPhase::ALL
            .iter()
            .map(|phase| phase.as_str())
            .collect::<Vec<_>>()
    );
    let timeline_phase = phases
        .iter()
        .find(|phase| phase.phase == ProcessingPhase::Timeline)
        .expect("automatic Timeline phase");
    let timeline_stats: serde_json::Value =
        serde_json::from_str(&timeline_phase.stats_json).expect("parse Timeline phase stats");
    let macb_inserted = timeline_stats["macbInsertedCount"]
        .as_u64()
        .expect("Timeline macbInsertedCount");
    let macb_total = timeline_stats["macbTotalCount"]
        .as_u64()
        .expect("Timeline macbTotalCount");
    assert!(
        macb_total > 0,
        "Timeline phase must retain MACB events once XFS timestamps are available: {timeline_stats}"
    );
    assert!(
        macb_total >= macb_inserted,
        "Timeline MACB totals are inconsistent: {timeline_stats}"
    );
    let search_phase = phases
        .iter()
        .find(|phase| phase.phase == ProcessingPhase::Search)
        .expect("automatic Search phase");
    let search_stats: serde_json::Value =
        serde_json::from_str(&search_phase.stats_json).expect("parse Search phase stats");
    let eligible = search_stats["eligibleCount"]
        .as_u64()
        .expect("Search eligibleCount");
    let indexed = search_stats["indexedCount"]
        .as_u64()
        .expect("Search indexedCount");
    let skipped = search_stats["skippedCount"]
        .as_u64()
        .expect("Search skippedCount");
    let failed = search_stats["failedCount"]
        .as_u64()
        .expect("Search failedCount");
    assert!(eligible > 0, "Search phase found no eligible files");
    assert!(indexed > 0, "Search phase indexed no files");
    assert_eq!(
        eligible,
        indexed + skipped + failed,
        "Search phase accounting is inconsistent: {search_stats}"
    );

    let source_conn = source_db::open_registered_source_db(case_conn, case_root, &source.id)
        .expect("open analyzed derived RBD source database");
    let completed_timeline_projections: u64 = source_conn
        .query_row(
            "SELECT COUNT(*)
             FROM timeline_projection_meta
             WHERE projection_key IN ('macb', 'macb_graph')
               AND status = 'done'",
            [],
            |row| row.get(0),
        )
        .expect("query completed Timeline projections");
    assert_eq!(
        completed_timeline_projections, 2,
        "MACB and timeline graph projections must both persist completion markers"
    );
    let linux_system_config_count = ArtifactRepo::new(&source_conn)
        .count_by_family()
        .expect("count derived VM artifact families")
        .into_iter()
        .find_map(|(family, count)| (family == "LinuxSystemConfig").then_some(count))
        .unwrap_or_default();
    assert!(
        linux_system_config_count > 0,
        "automatic finalization must persist LinuxSystemConfig artifacts"
    );
    let artifact_count = ArtifactRepo::new(&source_conn)
        .count()
        .expect("count automatic derived artifacts");
    let timeline_count = TimelineRepo::new(&source_conn)
        .count()
        .expect("count automatic derived timeline");
    assert!(artifact_count > 0, "automatic artifact projection is empty");
    assert!(timeline_count > 0, "automatic timeline projection is empty");

    let search = app_services::search_service::search_files_real(
        &source_db::source_index_dir(case_root, &source.id),
        "nologin",
        0,
        100,
    )
    .expect("query automatic derived search index");
    assert!(
        search.items.iter().any(|item| {
            item.path.replace('\\', "/").ends_with("etc/passwd")
                && item
                    .snippets
                    .iter()
                    .any(|snippet| snippet.text.contains("nologin"))
        }),
        "automatic search index cannot resolve retained /etc/passwd content"
    );

    let repeated = materialize_rbd_sources_for_cluster(case_conn, case_root, case_id, cluster_id)
        .expect("repeat ready derived-source materialization");
    for source in repeated {
        app_services::ceph_reconstruction::finalize_rbd_source_processing(
            case_conn,
            case_root,
            case_id,
            &source.data_source.id,
        )
        .expect("repeat ready derived-source processing");
    }
    let reopened = source_db::open_registered_source_db(case_conn, case_root, &source.id)
        .expect("reopen automatically processed derived source");
    assert_eq!(
        ArtifactRepo::new(&reopened)
            .count()
            .expect("recount derived artifacts"),
        artifact_count,
        "ready-source retry changed the artifact count"
    );
    assert_eq!(
        TimelineRepo::new(&reopened)
            .count()
            .expect("recount derived timeline"),
        timeline_count,
        "ready-source retry changed the timeline count"
    );
    let retried_phases = DataSourceProcessingPhaseRepo::new(case_conn)
        .list_for_data_source(&source.id)
        .expect("list retried derived-source phases");
    let retried_timeline = retried_phases
        .iter()
        .find(|phase| phase.phase == ProcessingPhase::Timeline)
        .expect("retried Timeline phase");
    let retried_search = retried_phases
        .iter()
        .find(|phase| phase.phase == ProcessingPhase::Search)
        .expect("retried Search phase");
    assert_eq!(
        retried_timeline.stats_json, timeline_phase.stats_json,
        "ready-source retry changed Timeline phase stats"
    );
    assert_eq!(
        retried_search.stats_json, search_phase.stats_json,
        "ready-source retry changed Search phase stats"
    );
}

fn assert_unique_source_storage(case_conn: &rusqlite::Connection, sources: &[DataSource]) {
    let mut data_source_ids = HashSet::new();
    let mut source_db_paths = HashSet::new();
    for source in sources {
        assert!(data_source_ids.insert(source.id.0.clone()));
        let storage = DataSourceRepo::new(case_conn)
            .find_storage(&source.id)
            .expect("query source storage")
            .expect("source storage");
        let rel_path = storage
            .source_db_rel_path
            .expect("source database relative path");
        assert!(source_db_paths.insert(rel_path));
    }
}

fn source_for_member<'a>(sources: &'a [DataSource], member_path: &Path) -> &'a DataSource {
    sources
        .iter()
        .find(|source| source.source_path == member_path)
        .unwrap_or_else(|| panic!("missing registered member {}", member_path.display()))
}

fn assert_member_metadata(
    case_conn: &rusqlite::Connection,
    source: &DataSource,
    plan: &LinuxClusterImportPlan,
    expected_index: u32,
) {
    let (cluster_id, member_index, member_count): (String, u32, u32) = case_conn
        .query_row(
            "SELECT cluster_id, cluster_member_index, cluster_member_count
             FROM data_sources WHERE id = ?1",
            [&source.id.0],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("query member metadata");
    assert_eq!(cluster_id, plan.cluster_id);
    assert_eq!(member_index, expected_index);
    assert_eq!(member_count as usize, PVE_MEMBER_COUNT);
}

fn assert_host_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    source: &DataSource,
) {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(&source.id)
        .expect("query host storage")
        .expect("host storage");
    assert_eq!(storage.import_state, "ready");
    let source_conn = source_db::open_registered_source_db(case_conn, case_root, &source.id)
        .expect("open host source database");
    assert_eq!(
        FileRepo::new(&source_conn)
            .count_by_data_source(&source.id)
            .expect("count host files"),
        host_file_count_oracle(source),
        "host file count differs for {}",
        source.name
    );
    for path in PVE_OS_FILES {
        assert_file_preview(
            case_conn,
            case_root,
            case_id,
            &source_conn,
            &source.id,
            path,
        );
    }
}

fn host_file_count_oracle(source: &DataSource) -> u64 {
    let file_name = source
        .source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match file_name.as_str() {
        "server01-disk01.e01" => 62_403,
        "server02-disk01.e01" => 62_380,
        "server03-disk01.e01" => 62_405,
        _ => panic!(
            "missing exact host file-count oracle for {}",
            source.source_path.display()
        ),
    }
}

fn assert_source_database_health(case_root: &Path, source: &DataSource) {
    let source_db_path = source_db::source_db_path(case_root, &source.id);
    assert!(source_db_path.is_file(), "missing retained source database");
    assert_sqlite_wal_quiescent(&source_db_path);
    {
        let source_conn = persistence_sqlite::open_existing_source(&source_db_path)
            .expect("open retained source database for health checks");
        let mut integrity_statement = source_conn
            .prepare("PRAGMA integrity_check")
            .expect("prepare source DB integrity_check");
        let integrity = integrity_statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("run source DB integrity_check")
            .collect::<Result<Vec<_>, _>>()
            .expect("read source DB integrity_check");
        assert_eq!(
            integrity,
            vec!["ok".to_string()],
            "source DB integrity_check failed for {}",
            source.name
        );
        drop(integrity_statement);

        let mut foreign_key_statement = source_conn
            .prepare("PRAGMA foreign_key_check")
            .expect("prepare source DB foreign_key_check");
        let foreign_key_violations = foreign_key_statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .expect("run source DB foreign_key_check")
            .collect::<Result<Vec<_>, _>>()
            .expect("read source DB foreign_key_check");
        assert!(
            foreign_key_violations.is_empty(),
            "source DB foreign_key_check failed for {}: {foreign_key_violations:?}",
            source.name
        );
        drop(foreign_key_statement);

        let checkpoint: (u32, u64, u64) = source_conn
            .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .expect("query source DB WAL convergence");
        assert_eq!(
            checkpoint,
            (0, 0, 0),
            "source DB WAL was not fully truncated for {}",
            source.name
        );
    }
    assert_sqlite_wal_quiescent(&source_db_path);
}

fn capture_rbd_parent_source_snapshots(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Vec<SourceDbReadOnlySnapshot> {
    let mut snapshots = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("query parent source databases")
        .into_iter()
        .filter(|source| source.kind != domain::DataSourceKind::CephRbd)
        .filter(|source| {
            DataSourceRepo::new(case_conn)
                .find_storage(&source.id)
                .expect("query parent source storage")
                .is_some_and(|storage| storage.import_state == "ready_metadata")
        })
        .map(|source| {
            let path = source_db::source_db_path(case_root, &source.id);
            source_db_read_only_snapshot(&source.id, &path)
        })
        .collect::<Vec<_>>();
    snapshots.sort_by(|left, right| left.source_id.cmp(&right.source_id));
    assert_eq!(
        snapshots.len(),
        PVE_RBD_REPLICA_COUNT,
        "retained RBD reconstruction requires exactly three metadata parent sources"
    );
    snapshots
}

fn assert_parent_source_snapshots_unchanged(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    expected: &[SourceDbReadOnlySnapshot],
) {
    let actual = capture_rbd_parent_source_snapshots(case_conn, case_root, case_id);
    assert_eq!(
        actual, expected,
        "RBD reconstruction changed a parent source database"
    );
}

fn source_db_read_only_snapshot(
    source_id: &DataSourceId,
    database_path: &Path,
) -> SourceDbReadOnlySnapshot {
    assert_sqlite_wal_quiescent(database_path);
    let metadata = std::fs::metadata(database_path).expect("read parent source DB metadata");
    assert!(metadata.len() > 0, "parent source DB must not be empty");
    SourceDbReadOnlySnapshot {
        source_id: source_id.0.clone(),
        length: metadata.len(),
        modified_at: metadata.modified().expect("read parent source DB mtime"),
        boundary_sha256: source_db_boundary_sha256(database_path, metadata.len()),
        full_sha256: std::env::var_os(PVE_RBD_DEEP_PARENT_HASH_ENV)
            .map(|_| infrastructure::hashing::sha256_file(database_path).expect("hash parent DB")),
    }
}

fn source_db_boundary_sha256(database_path: &Path, length: u64) -> String {
    const WINDOW: usize = 64 * 1024;
    let mut file = std::fs::File::open(database_path).expect("open parent source DB");
    let head_length = usize::try_from(length.min(WINDOW as u64)).expect("head length");
    let mut bytes = vec![0u8; head_length];
    file.read_exact(&mut bytes)
        .expect("read parent source DB head");
    if length > WINDOW as u64 {
        file.seek(SeekFrom::End(-(WINDOW as i64)))
            .expect("seek parent source DB tail");
        let mut tail = vec![0u8; WINDOW];
        file.read_exact(&mut tail)
            .expect("read parent source DB tail");
        bytes.extend_from_slice(&tail);
    }
    infrastructure::hashing::sha256_bytes(&bytes)
}

fn assert_sqlite_wal_quiescent(database_path: &Path) {
    let mut wal_path = database_path.as_os_str().to_os_string();
    wal_path.push("-wal");
    let wal_path = PathBuf::from(wal_path);
    if wal_path.exists() {
        assert_eq!(
            std::fs::metadata(&wal_path)
                .expect("read retained source database WAL metadata")
                .len(),
            0,
            "retained source database must not have pending WAL frames: {}",
            wal_path.display()
        );
    }
}

fn assert_file_preview(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    source_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    expected_path: &str,
) {
    let entry = find_file_by_linux_suffix(source_conn, data_source_id, expected_path)
        .unwrap_or_else(|| panic!("missing imported PVE host file {expected_path}"));
    let global_id = GlobalFileId::new(data_source_id.clone(), entry).encode().0;
    let handle = app_services::file_service::open_file_handle_for_case(
        case_conn, case_root, case_id, &global_id,
    )
    .unwrap_or_else(|error| panic!("open preview handle for {expected_path}: {error}"));
    let response = app_services::file_service::read_file_range_for_source_case(
        case_conn,
        case_root,
        case_id,
        &ViewerRangeRequestDto {
            handle_id: handle.handle_id,
            offset: 0,
            length: 512,
        },
    )
    .unwrap_or_else(|error| panic!("preview {expected_path}: {error}"));
    assert!(
        response.raw_bytes.is_some_and(|bytes| !bytes.is_empty()),
        "preview must return bytes for {expected_path}"
    );
}

fn find_file_by_linux_suffix(
    source_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    linux_path: &str,
) -> Option<FileEntryId> {
    let suffix = linux_path
        .trim_start_matches('/')
        .replace('\\', "/")
        .to_ascii_lowercase();
    let suffix_pattern = format!("%/{suffix}");
    source_conn
        .query_row(
            "SELECT id FROM file_entries
             WHERE data_source_id = ?1
               AND entry_type = 'file' COLLATE NOCASE
               AND (
                   LOWER(REPLACE(path, '\\', '/')) = ?2
                   OR LOWER(REPLACE(path, '\\', '/')) LIKE ?3
               )
             ORDER BY LENGTH(path) ASC
             LIMIT 1",
            rusqlite::params![data_source_id.0, suffix, suffix_pattern],
            |row| row.get::<_, String>(0).map(FileEntryId),
        )
        .ok()
}

fn assert_bluestore_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
) -> BluestoreSourceSummary {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(&source.id)
        .expect("query BlueStore storage")
        .expect("BlueStore storage");
    assert_eq!(
        storage.import_state, "ready_metadata",
        "BlueStore must be classified as metadata-only, not a POSIX filesystem"
    );
    assert!(storage.last_error.is_none());
    let source_db_path = source_db::source_db_path(case_root, &source.id);
    assert!(
        source_db_path.exists(),
        "failed member source database should remain available for diagnostics"
    );
    let source_conn =
        persistence_sqlite::open_existing_source(&source_db_path).expect("open failed source DB");
    assert_eq!(
        FileRepo::new(&source_conn)
            .count_by_data_source(&source.id)
            .expect("count BlueStore files"),
        0
    );
    let oracle = bluestore_oracle(source);
    let inventory: (
        String,
        Option<u32>,
        Option<String>,
        Option<i64>,
        u32,
        bool,
        u64,
    ) = source_conn
        .query_row(
            "SELECT osd_uuid, whoami, ceph_fsid, selected_epoch, valid_label_count,
                    osd_key_present, device_size
             FROM ceph_osd_inventory WHERE data_source_id = ?1",
            [&source.id.0],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .expect("query BlueStore inventory");
    assert_eq!(inventory.0, oracle.osd_uuid);
    assert_eq!(inventory.1, Some(oracle.osd_id));
    assert_eq!(inventory.2.as_deref(), Some(oracle.ceph_fsid));
    assert_eq!(inventory.3, Some(oracle.selected_epoch));
    assert!(inventory.4 >= 1);
    assert!(inventory.5);
    let bluefs: BluefsInventoryRow = source_conn
        .query_row(
            "SELECT inventory_id, bluefs_uuid, osd_uuid, sequence, block_size, crc32c,
                        shared_bdev, dedicated_db, dedicated_wal
                 FROM ceph_bluefs_superblocks WHERE data_source_id = ?1",
            [&source.id.0],
            |row| {
                Ok(BluefsInventoryRow {
                    inventory_id: row.get(0)?,
                    bluefs_uuid: row.get(1)?,
                    osd_uuid: row.get(2)?,
                    sequence: row.get(3)?,
                    block_size: row.get(4)?,
                    crc32c: row.get(5)?,
                    shared_bdev: row.get(6)?,
                    dedicated_db: row.get(7)?,
                    dedicated_wal: row.get(8)?,
                })
            },
        )
        .expect("query BlueFS superblock inventory");
    assert!(!bluefs.bluefs_uuid.is_empty());
    assert_eq!(bluefs.osd_uuid, inventory.0);
    assert_eq!(bluefs.bluefs_uuid, oracle.bluefs_uuid);
    assert_eq!(bluefs.osd_uuid, oracle.osd_uuid);
    assert_eq!(bluefs.sequence, 50);
    assert_eq!(bluefs.block_size, 4096);
    assert_eq!(bluefs.crc32c, oracle.crc32c);
    assert_eq!(bluefs.shared_bdev, Some(1));
    assert_eq!(bluefs.dedicated_db, Some(false));
    assert_eq!(bluefs.dedicated_wal, Some(false));

    let mut statement = source_conn
        .prepare(
            "SELECT device_id, offset, length
             FROM ceph_bluefs_log_extents
             WHERE inventory_id = (SELECT inventory_id FROM ceph_bluefs_superblocks
                                   WHERE data_source_id = ?1)
             ORDER BY ordinal",
        )
        .expect("prepare BlueFS extent query");
    let extents = statement
        .query_map([&source.id.0], |row| {
            Ok((
                row.get::<_, u8>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })
        .expect("query BlueFS extents")
        .collect::<Result<Vec<_>, _>>()
        .expect("load BlueFS extents");
    assert!(!extents.is_empty());
    assert_eq!(extents, vec![(1, oracle.extent_offset, 65_536)]);
    let (device_id, offset, length) = extents[0];
    assert_eq!(
        u32::from(device_id),
        bluefs.shared_bdev.expect("shared BlueFS bdev")
    );
    assert!(
        offset
            .checked_add(u64::from(length))
            .is_some_and(|end| end <= inventory.6),
        "BlueFS extent must remain inside the labeled device"
    );
    assert_bluefs_replay(&source_conn, &inventory.0, &oracle);
    assert_rocksdb_inventory(&source_conn, &source.id.0, &oracle);
    assert_bluestore_semantics(&source_conn, source, &bluefs.inventory_id, &oracle);

    BluestoreSourceSummary {
        osd_uuid: inventory.0,
        osd_id: inventory.1,
        ceph_fsid: inventory.2,
        bluefs_uuid: bluefs.bluefs_uuid,
    }
}

fn assert_bluestore_semantics(
    source_conn: &rusqlite::Connection,
    source: &DataSource,
    inventory_id: &str,
    oracle: &BluestoreOracle,
) {
    let scan = persisted_semantic_scan(source_conn, inventory_id);
    let latest_state = CephRocksdbLatestStateRepo::new(source_conn)
        .find(inventory_id)
        .expect("query BlueStore RocksDB latest state");
    assert_eq!(scan.schema_version, BLUESTORE_SEMANTIC_SCHEMA_VERSION);
    assert_eq!(scan.decode_profile, BLUESTORE_SEMANTIC_DECODE_PROFILE);
    assert!(scan.profile_complete);
    assert_eq!(
        scan.latest_state_sha256,
        latest_state_set_sha256(&latest_state),
        "semantic snapshot must bind to the persisted latest state"
    );
    assert_eq!(
        scan.collection_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_collections")
    );
    assert_eq!(
        scan.object_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_objects")
    );
    assert_eq!(
        scan.blob_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_blobs")
    );
    assert_eq!(
        scan.onode_shard_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_onode_shards")
    );
    assert_eq!(
        scan.logical_extent_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_logical_extents")
    );
    assert_eq!(
        scan.physical_extent_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_physical_extents")
    );
    assert_eq!(
        scan.checksum_chunk_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_checksum_chunks")
    );
    assert_eq!(
        scan.shared_blob_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_shared_blobs")
    );
    assert_eq!(
        scan.shared_ref_extent_count,
        semantic_table_count(source_conn, inventory_id, "ceph_bluestore_shared_blob_refs")
    );
    assert!(scan.object_count > 0);
    assert!(scan.blob_count > 0);
    assert!(scan.logical_extent_count > 0);
    assert!(scan.physical_extent_count > 0);
    assert!(scan.checksum_chunk_count > 0);
    assert!(scan.shared_blob_count > 0);
    assert!(scan.shared_ref_extent_count > 0);

    eprintln!(
        "BLUESTORE_SEMANTIC_ORACLE source={} semantic_sha256={} collections={} objects={} \
         blobs={} shards={} logical={} physical={} checksums={} shared={} shared_refs={}",
        source.name,
        scan.semantic_sha256,
        scan.collection_count,
        scan.object_count,
        scan.blob_count,
        scan.onode_shard_count,
        scan.logical_extent_count,
        scan.physical_extent_count,
        scan.checksum_chunk_count,
        scan.shared_blob_count,
        scan.shared_ref_extent_count,
    );
    let expected = &oracle.semantic;
    assert_eq!(scan.semantic_sha256, expected.semantic_sha256);
    assert_eq!(scan.collection_count, expected.collection_count);
    assert_eq!(scan.object_count, expected.object_count);
    assert_eq!(scan.blob_count, expected.blob_count);
    assert_eq!(scan.onode_shard_count, expected.onode_shard_count);
    assert_eq!(scan.logical_extent_count, expected.logical_extent_count);
    assert_eq!(scan.physical_extent_count, expected.physical_extent_count);
    assert_eq!(scan.checksum_chunk_count, expected.checksum_chunk_count);
    assert_eq!(scan.shared_blob_count, expected.shared_blob_count);
    assert_eq!(
        scan.shared_ref_extent_count,
        expected.shared_ref_extent_count
    );
}

struct PersistedSemanticScan {
    schema_version: u32,
    decode_profile: String,
    latest_state_sha256: String,
    semantic_sha256: String,
    collection_count: u64,
    object_count: u64,
    blob_count: u64,
    onode_shard_count: u64,
    logical_extent_count: u64,
    physical_extent_count: u64,
    checksum_chunk_count: u64,
    shared_blob_count: u64,
    shared_ref_extent_count: u64,
    profile_complete: bool,
}

fn persisted_semantic_scan(
    source_conn: &rusqlite::Connection,
    inventory_id: &str,
) -> PersistedSemanticScan {
    source_conn
        .query_row(
            "SELECT schema_version, decode_profile, latest_state_sha256,
                    semantic_sha256, collection_count, object_count, blob_count,
                    onode_shard_count, logical_extent_count, physical_extent_count,
                    checksum_chunk_count, shared_blob_count,
                    shared_ref_extent_count, profile_complete
             FROM ceph_bluestore_semantic_scans
             WHERE inventory_id = ?1",
            [inventory_id],
            |row| {
                Ok(PersistedSemanticScan {
                    schema_version: row.get(0)?,
                    decode_profile: row.get(1)?,
                    latest_state_sha256: row.get(2)?,
                    semantic_sha256: row.get(3)?,
                    collection_count: row.get(4)?,
                    object_count: row.get(5)?,
                    blob_count: row.get(6)?,
                    onode_shard_count: row.get(7)?,
                    logical_extent_count: row.get(8)?,
                    physical_extent_count: row.get(9)?,
                    checksum_chunk_count: row.get(10)?,
                    shared_blob_count: row.get(11)?,
                    shared_ref_extent_count: row.get(12)?,
                    profile_complete: row.get(13)?,
                })
            },
        )
        .expect("query BlueStore semantic scan")
}

fn semantic_table_count(
    source_conn: &rusqlite::Connection,
    inventory_id: &str,
    table: &str,
) -> u64 {
    source_conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE inventory_id = ?1"),
            [inventory_id],
            |row| row.get(0),
        )
        .unwrap_or_else(|error| panic!("query {table} row count: {error}"))
}

fn assert_rocksdb_inventory(
    source_conn: &rusqlite::Connection,
    data_source_id: &str,
    oracle: &BluestoreOracle,
) {
    let manifest = source_conn
        .query_row(
            "SELECT active_manifest_path, identity_uuid, manifest_file_number,
                    manifest_file_size, logical_edit_count, comparator_name,
                    last_sequence, next_file_number, log_number, prev_log_number,
                    max_column_family_id, min_log_number_to_keep
             FROM ceph_rocksdb_manifests
             WHERE data_source_id = ?1",
            [data_source_id],
            |row| {
                Ok(RocksDbManifestRow {
                    active_manifest_path: row.get(0)?,
                    identity_uuid: row.get(1)?,
                    manifest_file_number: row.get(2)?,
                    manifest_file_size: row.get(3)?,
                    logical_edit_count: row.get(4)?,
                    comparator_name: row.get(5)?,
                    last_sequence: row.get(6)?,
                    next_file_number: row.get(7)?,
                    log_number: row.get(8)?,
                    prev_log_number: row.get(9)?,
                    max_column_family_id: row.get(10)?,
                    min_log_number_to_keep: row.get(11)?,
                })
            },
        )
        .expect("query RocksDB manifest inventory");
    assert_eq!(manifest.active_manifest_path, oracle.manifest_path);
    assert_eq!(manifest.identity_uuid, oracle.rocksdb_identity);
    assert_eq!(manifest.manifest_file_number, oracle.manifest_file_number);
    assert_eq!(manifest.manifest_file_size, oracle.manifest_file_size);
    assert_eq!(manifest.logical_edit_count, 39);
    assert_eq!(manifest.comparator_name, "leveldb.BytewiseComparator");
    assert_eq!(manifest.last_sequence, oracle.rocksdb_last_sequence);
    assert_eq!(manifest.next_file_number, oracle.rocksdb_next_file_number);
    assert_eq!(manifest.log_number, oracle.rocksdb_log_number);
    assert_eq!(manifest.prev_log_number, 0);
    assert_eq!(manifest.max_column_family_id, 11);
    assert_eq!(
        manifest.min_log_number_to_keep,
        Some(oracle.rocksdb_log_number)
    );

    let column_families = load_rocksdb_column_families(source_conn, data_source_id);
    assert_eq!(
        column_families,
        vec![
            (0, "default".to_string()),
            (1, "m-0".to_string()),
            (2, "m-1".to_string()),
            (3, "m-2".to_string()),
            (4, "p-0".to_string()),
            (5, "p-1".to_string()),
            (6, "p-2".to_string()),
            (7, "O-0".to_string()),
            (8, "O-1".to_string()),
            (9, "O-2".to_string()),
            (10, "L".to_string()),
            (11, "P".to_string()),
        ]
    );
    let (live_count, missing_bluefs_files, invalid_metadata): (u64, u64, u64) = source_conn
        .query_row(
            "SELECT
                COUNT(*),
                SUM(CASE WHEN f.path IS NULL THEN 1 ELSE 0 END),
                SUM(CASE
                    WHEN l.path_id NOT BETWEEN 0 AND 3
                      OR l.format != 'newFile4'
                      OR l.smallest_sequence IS NULL
                      OR l.largest_sequence IS NULL
                      OR l.smallest_internal_key_length < 8
                      OR l.largest_internal_key_length < 8
                    THEN 1 ELSE 0 END)
             FROM ceph_rocksdb_live_files l
             JOIN ceph_rocksdb_manifests m ON m.inventory_id = l.inventory_id
             LEFT JOIN ceph_bluefs_files f
               ON f.inventory_id = l.inventory_id
              AND f.path = printf('db/%06d.sst', l.file_number)
             WHERE m.data_source_id = ?1",
            [data_source_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("query RocksDB live SST inventory");
    assert_eq!(live_count as usize, oracle.rocksdb_live_sst_count);
    assert_eq!(missing_bluefs_files, 0);
    assert_eq!(invalid_metadata, 0);

    let (bluefs_sst_count, missing_manifest_records): (u64, u64) = source_conn
        .query_row(
            "SELECT
                COUNT(*),
                SUM(CASE WHEN l.file_number IS NULL THEN 1 ELSE 0 END)
             FROM ceph_bluefs_files f
             JOIN ceph_rocksdb_manifests m ON m.inventory_id = f.inventory_id
             LEFT JOIN ceph_rocksdb_live_files l
               ON l.inventory_id = f.inventory_id
              AND f.path = printf('db/%06d.sst', l.file_number)
             WHERE m.data_source_id = ?1
               AND f.path GLOB 'db/[0-9][0-9][0-9][0-9][0-9][0-9]*.sst'
               AND substr(f.path, 4, length(f.path) - 7) NOT GLOB '*[^0-9]*'",
            [data_source_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("compare BlueFS SST files with the RocksDB live set");
    assert_eq!(bluefs_sst_count as usize, oracle.rocksdb_live_sst_count);
    assert_eq!(missing_manifest_records, 0);
    assert_sst_structure_inventory(source_conn, data_source_id, oracle);
    assert_wal_inventory(source_conn, data_source_id, oracle);
    let latest_state = load_latest_state_inventory(source_conn, data_source_id);
    assert_eq!(latest_state.len(), 12);
    assert_eq!(
        latest_state_oracle_sha256(&latest_state),
        oracle.rocksdb_latest_state_sha256,
        "per-column-family RocksDB latest-state oracle differs for OSD {}: {latest_state:#?}",
        oracle.osd_uuid,
    );
}

fn load_latest_state_inventory(
    source_conn: &rusqlite::Connection,
    data_source_id: &str,
) -> Vec<RocksDbLatestStateRow> {
    let mut statement = source_conn
        .prepare(
            "SELECT s.column_family_id, s.column_family_name,
                    s.point_mutation_count, s.sst_point_mutation_count,
                    s.wal_point_mutation_count, s.range_mutation_count,
                    s.sst_range_mutation_count, s.wal_range_mutation_count,
                    s.latest_value_count, s.deleted_key_count,
                    s.delete_decision_count, s.single_delete_decision_count,
                    s.range_delete_decision_count, s.merge_resolved_count,
                    s.merge_operand_count, s.range_hidden_version_count,
                    s.smallest_sequence, s.largest_sequence, s.sharding_sha256,
                    s.point_sha256, s.range_sha256, s.latest_state_sha256
             FROM ceph_rocksdb_latest_state s
             JOIN ceph_rocksdb_manifests m ON m.inventory_id = s.inventory_id
             WHERE m.data_source_id = ?1
             ORDER BY s.column_family_id",
        )
        .expect("prepare RocksDB latest-state query");
    statement
        .query_map([data_source_id], |row| {
            Ok(RocksDbLatestStateRow {
                column_family_id: row.get(0)?,
                column_family_name: row.get(1)?,
                point_mutation_count: row.get(2)?,
                sst_point_mutation_count: row.get(3)?,
                wal_point_mutation_count: row.get(4)?,
                range_mutation_count: row.get(5)?,
                sst_range_mutation_count: row.get(6)?,
                wal_range_mutation_count: row.get(7)?,
                latest_value_count: row.get(8)?,
                deleted_key_count: row.get(9)?,
                delete_decision_count: row.get(10)?,
                single_delete_decision_count: row.get(11)?,
                range_delete_decision_count: row.get(12)?,
                merge_resolved_count: row.get(13)?,
                merge_operand_count: row.get(14)?,
                range_hidden_version_count: row.get(15)?,
                smallest_sequence: row.get(16)?,
                largest_sequence: row.get(17)?,
                sharding_sha256: row.get(18)?,
                point_sha256: row.get(19)?,
                range_sha256: row.get(20)?,
                latest_state_sha256: row.get(21)?,
            })
        })
        .expect("query RocksDB latest-state rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("load RocksDB latest-state rows")
}

fn latest_state_oracle_sha256(rows: &[RocksDbLatestStateRow]) -> String {
    let mut canonical = String::from("meow.pve.rocksdb.latest-state-oracle.v1\n");
    for row in rows {
        writeln!(
            canonical,
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:?}|{:?}|{}|{}|{}|{}",
            row.column_family_id,
            row.column_family_name,
            row.point_mutation_count,
            row.sst_point_mutation_count,
            row.wal_point_mutation_count,
            row.range_mutation_count,
            row.sst_range_mutation_count,
            row.wal_range_mutation_count,
            row.latest_value_count,
            row.deleted_key_count,
            row.delete_decision_count,
            row.single_delete_decision_count,
            row.range_delete_decision_count,
            row.merge_resolved_count,
            row.merge_operand_count,
            row.range_hidden_version_count,
            row.smallest_sequence,
            row.largest_sequence,
            row.sharding_sha256,
            row.point_sha256,
            row.range_sha256,
            row.latest_state_sha256,
        )
        .expect("write latest-state oracle row");
    }
    infrastructure::hashing::sha256_bytes(canonical.as_bytes())
}

fn assert_wal_inventory(
    source_conn: &rusqlite::Connection,
    data_source_id: &str,
    oracle: &BluestoreOracle,
) {
    let actual: (u64, String, bool, u64, u32, u32, u64, u64, u64, u64) = source_conn
        .query_row(
            "SELECT w.wal_number, w.bluefs_path, w.post_manifest, w.file_size,
                    w.logical_record_count, w.empty_batch_count, w.mutation_count,
                    w.logical_payload_bytes, w.first_sequence, w.last_sequence
             FROM ceph_rocksdb_wal_files w
             JOIN ceph_rocksdb_manifests m ON m.inventory_id = w.inventory_id
             WHERE m.data_source_id = ?1",
            [data_source_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .expect("query RocksDB WAL inventory");
    assert_eq!(
        actual,
        (
            oracle.rocksdb_wal_number,
            oracle.wal_path.to_string(),
            false,
            oracle.rocksdb_wal_file_size,
            oracle.rocksdb_wal_record_count,
            oracle.rocksdb_wal_empty_batch_count,
            oracle.rocksdb_wal_mutation_count,
            oracle.rocksdb_wal_payload_bytes,
            oracle.rocksdb_wal_first_sequence,
            oracle.rocksdb_wal_last_sequence,
        )
    );
    let record_count: u32 = source_conn
        .query_row(
            "SELECT COUNT(*)
             FROM ceph_rocksdb_wal_records r
             JOIN ceph_rocksdb_manifests m ON m.inventory_id = r.inventory_id
             WHERE m.data_source_id = ?1",
            [data_source_id],
            |row| row.get(0),
        )
        .expect("count RocksDB WAL logical records");
    assert_eq!(record_count, oracle.rocksdb_wal_record_count);
}

fn assert_sst_structure_inventory(
    source_conn: &rusqlite::Connection,
    data_source_id: &str,
    oracle: &BluestoreOracle,
) {
    let (count, incomplete, invalid): (u64, u64, u64) = source_conn
        .query_row(
            "SELECT
                COUNT(*),
                SUM(CASE WHEN s.scan_complete = 0 THEN 1 ELSE 0 END),
                SUM(CASE
                    WHEN s.table_magic_hex != '88e241b785f4cff7'
                      OR s.format_version != 5
                      OR s.checksum_type != 'xxh3'
                      OR s.file_size != l.file_size
                      OR s.column_family_id != l.column_family_id
                      OR s.level != l.level
                      OR s.original_file_number != l.file_number
                      OR s.data_block_count = 0
                      OR s.entry_count = 0
                      OR s.key_space_summary_version != 1
                      OR json_extract(s.key_space_summary_json, '$.version') != 1
                      OR json_extract(s.key_space_summary_json, '$.complete') != 1
                    THEN 1 ELSE 0 END)
             FROM ceph_rocksdb_sst_inventory s
             JOIN ceph_rocksdb_live_files l
               ON l.inventory_id = s.inventory_id
              AND l.file_number = s.file_number
             JOIN ceph_rocksdb_manifests m ON m.inventory_id = s.inventory_id
             WHERE m.data_source_id = ?1",
            [data_source_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("query complete RocksDB SST structure inventory");
    assert_eq!(count as usize, oracle.rocksdb_live_sst_count);
    assert_eq!(incomplete, 0);
    assert_eq!(invalid, 0);
    let aggregate: (u64, u64) = source_conn
        .query_row(
            "SELECT COALESCE(SUM(s.data_block_count), 0),
                    COALESCE(SUM(s.entry_count), 0)
             FROM ceph_rocksdb_sst_inventory s
             JOIN ceph_rocksdb_manifests m ON m.inventory_id = s.inventory_id
             WHERE m.data_source_id = ?1",
            [data_source_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("query aggregate RocksDB SST structure inventory");
    assert_eq!(
        aggregate,
        (
            oracle.rocksdb_sst_data_block_count,
            oracle.rocksdb_sst_entry_count,
        ),
        "aggregate SST structure inventory differs for OSD {}",
        oracle.osd_uuid
    );
    if let Some(expected) = &oracle.representative_sst {
        let actual: (u64, u64, u64, u64, u64, u64, u64, u64) = source_conn
            .query_row(
                "SELECT data_block_count, entry_count, deletion_count,
                        raw_key_size, raw_value_size, data_size,
                        properties_index_size, filter_size
                 FROM ceph_rocksdb_sst_inventory s
                 JOIN ceph_rocksdb_manifests m ON m.inventory_id = s.inventory_id
                 WHERE m.data_source_id = ?1 AND s.file_number = ?2",
                rusqlite::params![data_source_id, expected.file_number],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .expect("query representative SST structure inventory");
        assert_eq!(
            actual,
            (
                expected.data_block_count,
                expected.entry_count,
                expected.deletion_count,
                expected.raw_key_size,
                expected.raw_value_size,
                expected.data_size,
                expected.index_size,
                expected.filter_size,
            )
        );
    }
}

fn load_rocksdb_column_families(
    source_conn: &rusqlite::Connection,
    data_source_id: &str,
) -> Vec<(u32, String)> {
    let mut statement = source_conn
        .prepare(
            "SELECT c.column_family_id, c.name
             FROM ceph_rocksdb_column_families c
             JOIN ceph_rocksdb_manifests m ON m.inventory_id = c.inventory_id
             WHERE m.data_source_id = ?1
               AND c.dropped = 0
               AND c.comparator_name = 'leveldb.BytewiseComparator'
             ORDER BY c.column_family_id",
        )
        .expect("prepare RocksDB column family query");
    statement
        .query_map([data_source_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query RocksDB column families")
        .collect::<Result<Vec<_>, _>>()
        .expect("load RocksDB column families")
}

fn assert_bluefs_replay(
    source_conn: &rusqlite::Connection,
    osd_uuid: &str,
    oracle: &BluestoreOracle,
) {
    let replay: (String, u32, u64, u64, u64, String) = source_conn
        .query_row(
            "SELECT r.inventory_id, r.transaction_count, r.first_sequence,
                    r.final_sequence, r.logical_bytes, r.stop_reason
             FROM ceph_bluefs_replays r
             JOIN ceph_bluefs_superblocks s ON s.inventory_id = r.inventory_id
             WHERE s.osd_uuid = ?1",
            [osd_uuid],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("query BlueFS replay summary");
    assert_eq!(replay.1, 4);
    assert_eq!(replay.2, 1);
    assert_eq!(replay.3, oracle.final_sequence);
    assert_eq!(replay.4, 0x22_000);
    assert_eq!(replay.5, "invalidTail");
    let directories = load_bluefs_paths(source_conn, "ceph_bluefs_directories", "path", &replay.0);
    assert_eq!(
        directories,
        vec![
            "ALLOCATOR_NCB_DIR".to_string(),
            "db".to_string(),
            "db.slow".to_string(),
            "db.wal".to_string(),
            "sharding".to_string(),
        ]
    );
    let files = load_bluefs_paths(source_conn, "ceph_bluefs_files", "path", &replay.0);
    assert_eq!(files.len(), oracle.file_count);
    assert!(files.iter().any(|path| path == "db/CURRENT"));
    assert!(files.iter().any(|path| path == oracle.manifest_path));
    assert!(files.iter().any(|path| path == oracle.wal_path));
    let sst_path = files
        .iter()
        .find(|path| path.ends_with(".sst"))
        .expect("BlueFS replay must contain an SST file");
    for path in [
        "db/CURRENT",
        oracle.manifest_path,
        oracle.wal_path,
        sst_path,
    ] {
        let extent_count: u64 = source_conn
            .query_row(
                "SELECT COUNT(*) FROM ceph_bluefs_file_extents
                 WHERE inventory_id = ?1 AND file_path = ?2",
                rusqlite::params![&replay.0, path],
                |row| row.get(0),
            )
            .expect("count representative BlueFS file extents");
        assert!(extent_count > 0, "BlueFS file {path} must retain extents");
    }
}

fn load_bluefs_paths(
    source_conn: &rusqlite::Connection,
    table: &str,
    column: &str,
    inventory_id: &str,
) -> Vec<String> {
    let sql = format!("SELECT {column} FROM {table} WHERE inventory_id = ?1 ORDER BY {column}");
    let mut statement = source_conn
        .prepare(&sql)
        .expect("prepare BlueFS path query");
    statement
        .query_map([inventory_id], |row| row.get(0))
        .expect("query BlueFS paths")
        .collect::<Result<Vec<_>, _>>()
        .expect("load BlueFS paths")
}

fn bluestore_oracle(source: &DataSource) -> BluestoreOracle {
    let file_name = source
        .source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match file_name.as_str() {
        "server01-disk02.e01" => BluestoreOracle {
            osd_id: 0,
            osd_uuid: "9630c2a5-650a-4395-a47a-ec496515bd61",
            selected_epoch: 23,
            ceph_fsid: PVE_CLUSTER_FSID,
            bluefs_uuid: "394d12df-4023-44dc-b4c5-10b5e5dd48f4",
            crc32c: 0x1b31_b14c,
            extent_offset: 353_427_456,
            final_sequence: 186_890,
            file_count: 44,
            manifest_path: "db/MANIFEST-000143",
            wal_path: "db.wal/000142.log",
            rocksdb_identity: "318c61d3-7d8b-497a-b02a-d3683123595d",
            manifest_file_number: 143,
            manifest_file_size: 7280,
            rocksdb_live_sst_count: 35,
            rocksdb_next_file_number: 148,
            rocksdb_last_sequence: 1_077_117,
            rocksdb_log_number: 127,
            rocksdb_sst_data_block_count: 9_994,
            rocksdb_sst_entry_count: 159_439,
            rocksdb_wal_number: 142,
            rocksdb_wal_file_size: 3_921_274,
            rocksdb_wal_record_count: 3_710,
            rocksdb_wal_empty_batch_count: 1_107,
            rocksdb_wal_mutation_count: 9_338,
            rocksdb_wal_payload_bytes: 3_894_471,
            rocksdb_wal_first_sequence: 1_077_118,
            rocksdb_wal_last_sequence: 1_086_455,
            rocksdb_latest_state_sha256:
                "b4f31e224ff485b29b1b3ac7c21e079344250bf37a954b304d43294b1da22eed",
            semantic: BluestoreSemanticOracle {
                semantic_sha256: "794ab1ea6632d809bac456d9cd5e5e54c3a46b93977d2224f98c0d564a46c73b",
                collection_count: 34,
                object_count: 2924,
                blob_count: 116_135,
                onode_shard_count: 18_971,
                logical_extent_count: 116_487,
                physical_extent_count: 134_148,
                checksum_chunk_count: 1_839_658,
                shared_blob_count: 23_316,
                shared_ref_extent_count: 27_897,
            },
            representative_sst: Some(RepresentativeSstOracle {
                file_number: 146,
                data_block_count: 148,
                entry_count: 23_364,
                deletion_count: 0,
                raw_key_size: 420_609,
                raw_value_size: 298_145,
                data_size: 245_834,
                index_size: 3_106,
                filter_size: 58_437,
            }),
        },
        "server02-disk02.e01" => BluestoreOracle {
            osd_id: 1,
            osd_uuid: "de8554de-f932-448d-be2c-0474df6c16c5",
            selected_epoch: 21,
            ceph_fsid: PVE_CLUSTER_FSID,
            bluefs_uuid: "e1b8a63e-3c93-4743-8232-b236b82fec83",
            crc32c: 0x17d5_c472,
            extent_offset: 352_542_720,
            final_sequence: 185_969,
            file_count: 49,
            manifest_path: "db/MANIFEST-000121",
            wal_path: "db.wal/000120.log",
            rocksdb_identity: "15f9cf98-cb4f-4d78-9d94-ae6235eb075b",
            manifest_file_number: 121,
            manifest_file_size: 8321,
            rocksdb_live_sst_count: 40,
            rocksdb_next_file_number: 126,
            rocksdb_last_sequence: 1_052_658,
            rocksdb_log_number: 105,
            rocksdb_sst_data_block_count: 10_152,
            rocksdb_sst_entry_count: 160_791,
            rocksdb_wal_number: 120,
            rocksdb_wal_file_size: 4_142_839,
            rocksdb_wal_record_count: 3_782,
            rocksdb_wal_empty_batch_count: 1_084,
            rocksdb_wal_mutation_count: 9_644,
            rocksdb_wal_payload_bytes: 4_115_489,
            rocksdb_wal_first_sequence: 1_052_659,
            rocksdb_wal_last_sequence: 1_062_302,
            rocksdb_latest_state_sha256:
                "0cf9b7ead1e5953fa84f1c57a16be4f1a2d5fd4713d2ed1ad20cf8cf9d320880",
            semantic: BluestoreSemanticOracle {
                semantic_sha256: "441e1a48ec5ca51e5ff2caa94eac106d283d9375bbbc08d841196eb84fbe78e9",
                collection_count: 34,
                object_count: 2927,
                blob_count: 116_135,
                onode_shard_count: 18_970,
                logical_extent_count: 116_487,
                physical_extent_count: 134_154,
                checksum_chunk_count: 1_839_666,
                shared_blob_count: 23_316,
                shared_ref_extent_count: 27_900,
            },
            representative_sst: None,
        },
        "server03-disk02.e01" => BluestoreOracle {
            osd_id: 2,
            osd_uuid: "cd6f9b5c-37d5-4dc0-8588-9669d156b02c",
            selected_epoch: 22,
            ceph_fsid: PVE_CLUSTER_FSID,
            bluefs_uuid: "d8f0162e-aefe-4397-ad64-16b28af988a1",
            crc32c: 0x7838_a645,
            extent_offset: 156_147_712,
            final_sequence: 185_678,
            file_count: 42,
            manifest_path: "db/MANIFEST-000128",
            wal_path: "db.wal/000127.log",
            rocksdb_identity: "8024bc80-69cc-4adc-9f00-364b295f5312",
            manifest_file_number: 128,
            manifest_file_size: 6662,
            rocksdb_live_sst_count: 33,
            rocksdb_next_file_number: 132,
            rocksdb_last_sequence: 1_061_239,
            rocksdb_log_number: 110,
            rocksdb_sst_data_block_count: 9_954,
            rocksdb_sst_entry_count: 158_744,
            rocksdb_wal_number: 127,
            rocksdb_wal_file_size: 4_145_432,
            rocksdb_wal_record_count: 3_812,
            rocksdb_wal_empty_batch_count: 1_112,
            rocksdb_wal_mutation_count: 9_644,
            rocksdb_wal_payload_bytes: 4_117_873,
            rocksdb_wal_first_sequence: 1_061_240,
            rocksdb_wal_last_sequence: 1_070_883,
            rocksdb_latest_state_sha256:
                "32d7af9d9eda6ca168cb9a85a7b17a36c9fce012f9301b354aebb1b633bee978",
            semantic: BluestoreSemanticOracle {
                semantic_sha256: "d5eb02ba6e77a66476a2c84f010bca75ec77d870858d15e6b57681fb075028bc",
                collection_count: 34,
                object_count: 2930,
                blob_count: 116_135,
                onode_shard_count: 18_974,
                logical_extent_count: 116_487,
                physical_extent_count: 134_150,
                checksum_chunk_count: 1_839_646,
                shared_blob_count: 23_316,
                shared_ref_extent_count: 27_911,
            },
            representative_sst: None,
        },
        _ => panic!(
            "missing exact BlueStore oracle for {}",
            source.source_path.display()
        ),
    }
}

fn is_host_disk(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("-disk01"))
}
