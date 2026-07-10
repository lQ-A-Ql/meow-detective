//! Real-sample dual data source import isolation regression.
//!
//! Run explicitly with:
//! `cargo test -p forensics-desktop --test dual_source_import -- --ignored --nocapture`

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_pipeline::{execute_import_job_with_counts, ImportJobOptions};
use app_services::source_db::{self, GlobalFileId};
use domain::{CaseId, CaseMeta, DataSourceId, FileEntryId};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo, job_repo::JobRepo,
};
use tempfile::TempDir;
use transport::commands::{ImportDataSourceRequest, ImportTargetPlatformDto};
use transport::dto::ViewerRangeRequestDto;

const WINDOWS_E01: &str = r"D:\獬豸杯\检材2.E01";
const LINUX_E01: &str = r"D:\獬豸杯\检材3.E01";

#[test]
#[ignore = "requires real Windows/Linux E01 fixtures and performs full serial imports"]
fn real_samples_import_into_isolated_source_databases_serially() {
    assert_fixture_exists(WINDOWS_E01);
    assert_fixture_exists(LINUX_E01);

    let temp = TempDir::new().expect("temp case root");
    let case_root = temp.path().join("dual-source-isolation-case");
    std::fs::create_dir_all(&case_root).expect("create case root");

    let case_id = CaseId("dual-source-isolation".to_string());
    let case_conn = persistence_sqlite::connection::open_or_create(&case_root.join("app.db"))
        .expect("open app db");
    persistence_sqlite::runner::run_all(&case_conn).expect("run app migrations");
    CaseRepo::new(&case_conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Dual Source Isolation".to_string(),
            number: None,
            examiner: Some("real-sample-regression".to_string()),
            notes: Some(
                "Serial Windows + Linux E01 import isolation regression fixture".to_string(),
            ),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("insert case");

    let windows_ds = import_fixture_serially(
        &case_conn,
        &case_root,
        &case_id,
        WINDOWS_E01,
        ImportTargetPlatformDto::Windows,
        "windows-e01",
    );
    assert_source_storage(&case_conn, &case_root, &windows_ds, "windows");
    assert_app_db_does_not_store_file_tree(&case_conn);

    let linux_ds = import_fixture_serially(
        &case_conn,
        &case_root,
        &case_id,
        LINUX_E01,
        ImportTargetPlatformDto::Linux,
        "linux-e01",
    );
    assert_source_storage(&case_conn, &case_root, &linux_ds, "linux");
    assert_app_db_does_not_store_file_tree(&case_conn);

    assert_ne!(windows_ds, linux_ds, "data sources must remain distinct");
    assert_case_data_sources(&case_conn, &case_root, &case_id, &windows_ds, &linux_ds);
    assert_file_tree_aggregates_source_scoped_roots(
        &case_conn,
        &case_root,
        &case_id,
        &windows_ds,
        &linux_ds,
    );
    assert_preview_smoke(&case_conn, &case_root, &case_id, &windows_ds);
    assert_preview_smoke(&case_conn, &case_root, &case_id, &linux_ds);
    assert_source_scoped_analysis_ids(&case_conn, &case_root, &case_id);
}

fn import_fixture_serially(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    source_path: &str,
    platform: ImportTargetPlatformDto,
    profile: &str,
) -> DataSourceId {
    let before = data_source_ids(case_conn, case_id);
    let request = ImportDataSourceRequest {
        source_path: source_path.to_string(),
        source_kind: Default::default(),
        platform: Some(platform),
        profile: Some(profile.to_string()),
    };
    let config =
        app_services::import_precheck::prepare_import_source_config(&request).expect("precheck");
    let job_id = JobRepo::new(case_conn)
        .create(&case_id.0, "import")
        .expect("create import job");
    let cancel_token = Arc::new(AtomicBool::new(false));
    let options = ImportJobOptions {
        event_sink: None,
        cancel_token: &cancel_token,
        max_import_workers: Some(1),
        max_analysis_workers: Some(1),
        analysis_mode: ImportAnalysisMode::MetadataOnly,
    };

    execute_import_job_with_counts(case_conn, case_id, case_root, config, &job_id, options)
        .expect("serial import should complete");

    let after = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("list data sources");
    let created = after
        .into_iter()
        .filter(|source| !before.contains(&source.id.0))
        .collect::<Vec<_>>();
    assert_eq!(
        created.len(),
        1,
        "each serial import should register exactly one new data source"
    );
    created[0].id.clone()
}

fn assert_fixture_exists(path: &str) {
    let path = Path::new(path);
    assert!(path.exists(), "fixture missing: {}", path.display());
    assert!(path.is_file(), "fixture is not a file: {}", path.display());
}

fn data_source_ids(case_conn: &rusqlite::Connection, case_id: &CaseId) -> HashSet<String> {
    DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("list data sources")
        .into_iter()
        .map(|source| source.id.0)
        .collect()
}

fn assert_source_storage(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    expected_platform: &str,
) {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)
        .expect("load source storage")
        .expect("source storage row");
    assert_eq!(storage.storage_model, "source_db");
    assert_eq!(storage.platform, expected_platform);
    assert_eq!(storage.import_state, "ready");
    assert_eq!(
        storage.source_db_rel_path.as_deref(),
        Some(format!("sources/{}/source.db", data_source_id.0).as_str())
    );

    let source_db = source_db::source_db_path(case_root, data_source_id);
    assert!(
        source_db.exists(),
        "source DB missing: {}",
        source_db.display()
    );

    let source_conn = source_db::open_registered_source_db(case_conn, case_root, data_source_id)
        .expect("open registered source db");
    let local_sources = DataSourceRepo::new(&source_conn)
        .find_by_case(&CaseId("dual-source-isolation".to_string()))
        .expect("load source-local metadata");
    assert_eq!(local_sources.len(), 1);
    assert_eq!(local_sources[0].id, *data_source_id);

    let file_count = FileRepo::new(&source_conn)
        .count_by_data_source(data_source_id)
        .expect("count source files");
    assert!(file_count > 0, "source DB has no file entries");
}

fn assert_app_db_does_not_store_file_tree(case_conn: &rusqlite::Connection) {
    let app_file_entries: i64 = case_conn
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .expect("count app file entries");
    assert_eq!(
        app_file_entries, 0,
        "app.db must remain a control database; file tree rows belong in source.db"
    );
}

fn assert_case_data_sources(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    windows_ds: &DataSourceId,
    linux_ds: &DataSourceId,
) {
    let summaries =
        app_services::file_service::get_data_sources_for_case(case_conn, case_root, case_id)
            .expect("get case data source summaries");
    assert_eq!(summaries.len(), 2);

    let windows = summaries
        .iter()
        .find(|source| source.id == windows_ds.0)
        .expect("windows source summary");
    assert_eq!(windows.platform.as_deref(), Some("windows"));
    assert_eq!(windows.storage_model.as_deref(), Some("source_db"));
    assert!(windows.file_count.unwrap_or_default() > 0);

    let linux = summaries
        .iter()
        .find(|source| source.id == linux_ds.0)
        .expect("linux source summary");
    assert_eq!(linux.platform.as_deref(), Some("linux"));
    assert_eq!(linux.storage_model.as_deref(), Some("source_db"));
    assert!(linux.file_count.unwrap_or_default() > 0);
}

fn assert_file_tree_aggregates_source_scoped_roots(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    windows_ds: &DataSourceId,
    linux_ds: &DataSourceId,
) {
    let roots =
        app_services::file_service::get_file_tree_for_case(case_conn, case_root, case_id, false)
            .expect("get file tree");
    assert!(
        !roots.is_empty(),
        "case file tree should expose source roots"
    );
    assert!(
        roots.iter().all(|node| node.id.starts_with("ds:")),
        "all root ids must be source-scoped"
    );
    assert!(
        roots
            .iter()
            .any(|node| node.id.starts_with(&format!("ds:{}:", windows_ds.0))),
        "missing Windows source roots"
    );
    assert!(
        roots
            .iter()
            .any(|node| node.id.starts_with(&format!("ds:{}:", linux_ds.0))),
        "missing Linux source roots"
    );
}

fn assert_preview_smoke(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) {
    let source_conn = source_db::open_registered_source_db(case_conn, case_root, data_source_id)
        .expect("open source db");
    let candidates = previewable_file_ids(&source_conn, data_source_id);
    assert!(
        !candidates.is_empty(),
        "source DB has no positive-size file candidates"
    );

    let mut failures = Vec::new();
    for local_file_id in candidates {
        let global_file_id = GlobalFileId::new(data_source_id.clone(), local_file_id.clone())
            .encode()
            .0;
        let handle = match app_services::file_service::open_file_handle_for_case(
            case_conn,
            case_root,
            &case_id.0,
            &global_file_id,
        ) {
            Ok(handle) => handle,
            Err(error) => {
                failures.push(format!("{}: open: {error}", local_file_id.0));
                continue;
            }
        };
        assert!(handle.handle_id.starts_with("file:ds:"));

        let mut request = ViewerRangeRequestDto {
            handle_id: handle.handle_id,
            offset: 0,
            length: 16,
        };
        request.validate().expect("viewer range request is valid");
        match app_services::file_service::read_file_range_for_source_case(
            case_conn, case_root, &case_id.0, &request,
        ) {
            Ok(response) if response.raw_bytes.is_some() || !response.lines.is_empty() => return,
            Ok(_) => failures.push(format!("{}: empty preview response", local_file_id.0)),
            Err(error) => failures.push(format!("{}: range: {error}", local_file_id.0)),
        }
    }

    panic!(
        "no previewable file resolved for source {}; first failures: {}",
        data_source_id.0,
        failures.into_iter().take(8).collect::<Vec<_>>().join(" | ")
    );
}

fn previewable_file_ids(
    source_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
) -> Vec<FileEntryId> {
    let mut stmt = source_conn
        .prepare(
            "SELECT f.id
             FROM file_entries AS f
             WHERE f.data_source_id = ?1
               AND lower(f.entry_type) = 'file'
               AND COALESCE(f.size, 0) > 0
               AND (
                   f.parent_id IS NULL
                   OR EXISTS (
                       SELECT 1
                       FROM file_entries AS p
                       WHERE p.id = f.parent_id
                         AND p.data_source_id = f.data_source_id
                   )
               )
             ORDER BY
               CASE WHEN f.parent_id IS NULL THEN 1 ELSE 0 END,
               COALESCE(f.size, 0) ASC,
               f.path ASC
             LIMIT 128",
        )
        .expect("prepare preview candidate query");
    let rows = stmt
        .query_map([data_source_id.0.as_str()], |row| {
            Ok(FileEntryId(row.get::<_, String>(0)?))
        })
        .expect("query preview candidates");
    rows.collect::<Result<Vec<_>, _>>()
        .expect("collect preview candidates")
}

fn assert_source_scoped_analysis_ids(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
) {
    let artifacts = app_services::artifact_service::get_artifact_rows_for_case(
        case_conn, case_root, case_id, None,
    )
    .expect("get artifact rows");
    assert!(
        artifacts
            .iter()
            .all(|artifact| artifact.id.starts_with("ds:")),
        "artifact rows returned from case-level APIs must be source-scoped"
    );

    let timeline = app_services::timeline_service::query_timeline_for_case(
        case_conn, case_root, case_id, 0, 100,
    )
    .expect("get timeline rows");
    assert!(
        timeline
            .items
            .iter()
            .all(|event| event.id.starts_with("ds:") && event.source_object_id.starts_with("ds:")),
        "timeline rows returned from case-level APIs must be source-scoped"
    );

    let correlation =
        app_services::correlation::get_correlation_snapshot_for_case(case_conn, case_root, case_id)
            .expect("get correlation snapshot");
    assert!(
        correlation
            .leads
            .iter()
            .all(|lead| lead.primary_file_id.starts_with("ds:")),
        "correlation lead file ids must be source-scoped"
    );
}
