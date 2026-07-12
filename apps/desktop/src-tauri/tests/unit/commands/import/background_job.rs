use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::cluster_service::{plan_linux_cluster_import, LinuxClusterImportPlan};
use app_services::import_analysis::ImportAnalysisMode;
use app_services::source_db::{self, GlobalFileId};
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, FileEntryId};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo, datasource_cluster_repo::DataSourceClusterRepo,
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
    assert_eq!(job.status, cluster.import_state);
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
    let mut failed_count = 0;
    for member in &plan.members {
        let source = source_for_member(&sources, &member.source_path);
        assert_member_metadata(case_conn, source, plan, member.member_index);
        let storage = DataSourceRepo::new(case_conn)
            .find_storage(&source.id)
            .expect("query member storage")
            .expect("member storage");
        match storage.import_state.as_str() {
            "ready" => ready_count += 1,
            "failed" => failed_count += 1,
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
            assert_bluestore_source(case_conn, case_root, source);
        }
    }
    assert_eq!(ready_count, PVE_HOST_COUNT);
    assert_eq!(failed_count, PVE_MEMBER_COUNT - PVE_HOST_COUNT);
    assert_eq!(cluster.member_count as usize, PVE_MEMBER_COUNT);
    assert_eq!(cluster.ready_count as usize, ready_count);
    assert_eq!(cluster.failed_count as usize, failed_count);
    assert_eq!(cluster.import_state, "failed");
    assert!(cluster.last_error.is_some());
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
) {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(&source.id)
        .expect("query BlueStore storage")
        .expect("BlueStore storage");
    assert_eq!(
        storage.import_state, "failed",
        "BlueStore must not masquerade as a ready POSIX file system"
    );
    let last_error = storage
        .last_error
        .as_deref()
        .expect("BlueStore failure must preserve the explicit reason");
    assert!(
        last_error.contains("CEPH_BLUESTORE_UNSUPPORTED")
            && last_error.contains("Ceph BlueStore OSD block device detected")
            && last_error.contains("RADOS/PG/object reconstruction is not supported"),
        "unexpected BlueStore failure: {last_error}"
    );
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
}

fn is_host_disk(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("-disk01"))
}
