use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::cluster_service::{plan_linux_cluster_import, LinuxClusterImportPlan};
use app_services::import_analysis::ImportAnalysisMode;
use app_services::source_db::{self, GlobalFileId};
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, FileEntryId};
use persistence_sqlite::repositories::{
    audit_repo::AuditRepo, case_repo::CaseRepo, datasource_cluster_repo::DataSourceClusterRepo,
    datasource_repo::DataSourceRepo, file_repo::FileRepo, job_repo::JobRepo,
};
use tempfile::TempDir;
use transport::dto::ViewerRangeRequestDto;

use super::{run_background_linux_cluster_import_job, BackgroundLinuxClusterImportJob};

const PVE_CLUSTER_ROOT_ENV: &str = "FORENSICS_PVE_CLUSTER_ROOT";
const PVE_MEMBER_COUNT: usize = 6;
const PVE_HOST_COUNT: usize = 3;
const PVE_OS_FILES: &[&str] = &[
    "/etc/passwd",
    "/etc/os-release",
    "/etc/hostname",
    "/var/lib/pve-cluster/config.db",
];

struct BluestoreSourceSummary {
    osd_uuid: String,
    osd_id: Option<u32>,
    ceph_fsid: Option<String>,
    bluefs_uuid: String,
}

struct BluestoreOracle {
    osd_uuid: &'static str,
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
    representative_sst: Option<RepresentativeSstOracle>,
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

#[test]
#[ignore = "requires the private six-member PVE E01 cluster fixture"]
fn real_pve_cluster_import_attempts_every_member_and_isolates_source_databases() {
    let fixture_root = required_fixture_root();
    let plan = plan_linux_cluster_import(&fixture_root, Some("pve-cluster".to_string()))
        .expect("plan PVE cluster import");
    assert_plan(&fixture_root, &plan);

    let temp = TempDir::new().expect("create temporary case root");
    let case_root = temp.path().join("pve-cluster-case");
    std::fs::create_dir_all(&case_root).expect("create case directory");
    let case_id = CaseId("pve-cluster-import-regression".to_string());
    let case_conn = create_case_database(&case_root, &case_id);
    let job_id = JobRepo::new(&case_conn)
        .create(&case_id.0, "linux-cluster-import")
        .expect("create cluster import job");
    drop(case_conn);

    let result = run_background_linux_cluster_import_job(
        BackgroundLinuxClusterImportJob {
            db_path: case_root.join("app.db"),
            case_id: case_id.clone(),
            case_root: case_root.clone(),
            plan: plan.clone(),
            job_id: job_id.clone(),
            max_import_workers: Some(1),
            max_analysis_workers: Some(1),
            analysis_mode: ImportAnalysisMode::MetadataOnly,
        },
        None,
        Arc::new(AtomicBool::new(false)),
    );
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
    assert_job_outcome(&case_conn, &job_id, &cluster);
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
    assert_eq!(
        plan.members
            .iter()
            .map(|member| member.member_index)
            .collect::<Vec<_>>(),
        (0..PVE_MEMBER_COUNT as u32).collect::<Vec<_>>()
    );
    assert_eq!(
        plan.members
            .iter()
            .filter(|member| is_host_disk(&member.source_path))
            .count(),
        PVE_HOST_COUNT
    );
    assert!(plan.members.iter().all(|member| member
        .source_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("e01"))));
}

fn create_case_database(case_root: &Path, case_id: &CaseId) -> rusqlite::Connection {
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
    assert_eq!(
        sources.len(),
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
        let source = source_for_member(&sources, &member.source_path);
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
    }
    assert_eq!(ready_count, PVE_HOST_COUNT);
    assert_eq!(metadata_count, PVE_MEMBER_COUNT - PVE_HOST_COUNT);
    assert_eq!(osd_ids, HashSet::from([0, 1, 2]));
    assert_eq!(osd_uuids.len(), 3);
    assert_eq!(bluefs_uuids.len(), 3);
    assert_eq!(cluster_fsids.len(), 1);
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
    assert!(
        FileRepo::new(&source_conn)
            .count_by_data_source(&source.id)
            .expect("count host files")
            >= 50_000
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
    let inventory: (String, Option<u32>, Option<String>, u32, bool, u64) = source_conn
        .query_row(
            "SELECT osd_uuid, whoami, ceph_fsid, valid_label_count, osd_key_present,
                    device_size
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
                ))
            },
        )
        .expect("query BlueStore inventory");
    assert!(!inventory.0.is_empty());
    assert!(inventory.1.is_some());
    assert!(inventory.2.is_some());
    assert!(inventory.3 >= 1);
    assert!(inventory.4);
    let bluefs: BluefsInventoryRow = source_conn
        .query_row(
            "SELECT bluefs_uuid, osd_uuid, sequence, block_size, crc32c,
                        shared_bdev, dedicated_db, dedicated_wal
                 FROM ceph_bluefs_superblocks WHERE data_source_id = ?1",
            [&source.id.0],
            |row| {
                Ok(BluefsInventoryRow {
                    bluefs_uuid: row.get(0)?,
                    osd_uuid: row.get(1)?,
                    sequence: row.get(2)?,
                    block_size: row.get(3)?,
                    crc32c: row.get(4)?,
                    shared_bdev: row.get(5)?,
                    dedicated_db: row.get(6)?,
                    dedicated_wal: row.get(7)?,
                })
            },
        )
        .expect("query BlueFS superblock inventory");
    let oracle = bluestore_oracle(source);
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
            .is_some_and(|end| end <= inventory.5),
        "BlueFS extent must remain inside the labeled device"
    );
    assert_bluefs_replay(&source_conn, &inventory.0, &oracle);
    assert_rocksdb_inventory(&source_conn, &source.id.0, &oracle);

    BluestoreSourceSummary {
        osd_uuid: inventory.0,
        osd_id: inventory.1,
        ceph_fsid: inventory.2,
        bluefs_uuid: bluefs.bluefs_uuid,
    }
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
            osd_uuid: "9630c2a5-650a-4395-a47a-ec496515bd61",
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
            osd_uuid: "de8554de-f932-448d-be2c-0474df6c16c5",
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
            representative_sst: None,
        },
        "server03-disk02.e01" => BluestoreOracle {
            osd_uuid: "cd6f9b5c-37d5-4dc0-8588-9669d156b02c",
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
