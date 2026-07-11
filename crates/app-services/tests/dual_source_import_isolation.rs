use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_pipeline::{execute_import_job_with_counts, ImportJobOptions};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};

#[test]
fn windows_and_linux_logical_imports_are_isolated_in_both_orders() {
    for linux_first in [false, true] {
        run_order_isolation_case(linux_first);
    }
}

fn run_order_isolation_case(linux_first: bool) {
    let temp = tempfile::TempDir::new().expect("create dual-source fixture root");
    let windows_path = temp.path().join("windows-source");
    let linux_path = temp.path().join("linux-source");
    write_fixture(
        &windows_path,
        "Windows/System32/drivers/etc/hosts",
        b"127.0.0.1 localhost",
    );
    write_fixture(
        &linux_path,
        "etc/os-release",
        b"ID=test-linux\nNAME=Test Linux\n",
    );

    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        if linux_first {
            "linux-then-windows"
        } else {
            "windows-then-linux"
        },
        Some("stage2-isolation"),
    )
    .expect("create isolated case");

    active
        .with_conn(|case_conn| {
            let (windows_id, linux_id) = if linux_first {
                let linux_id =
                    import_source(case_conn, &active, &linux_path, DataSourcePlatform::Linux)?;
                let windows_id = import_source(
                    case_conn,
                    &active,
                    &windows_path,
                    DataSourcePlatform::Windows,
                )?;
                (windows_id, linux_id)
            } else {
                let windows_id = import_source(
                    case_conn,
                    &active,
                    &windows_path,
                    DataSourcePlatform::Windows,
                )?;
                let linux_id =
                    import_source(case_conn, &active, &linux_path, DataSourcePlatform::Linux)?;
                (windows_id, linux_id)
            };

            assert_registered_platform(case_conn, &windows_id, "windows");
            assert_registered_platform(case_conn, &linux_id, "linux");
            assert_source_db_is_local(&active.case_root, &windows_id)?;
            assert_source_db_is_local(&active.case_root, &linux_id)?;

            let app_file_count: i64 =
                case_conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
            assert_eq!(app_file_count, 0, "app.db must remain a control database");

            let summaries = app_services::file_service::get_data_sources_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(summaries.len(), 2);
            assert!(summaries
                .iter()
                .any(|source| source.id == windows_id.0 && source.platform == "windows"));
            assert!(summaries
                .iter()
                .any(|source| source.id == linux_id.0 && source.platform == "linux"));
            Ok(())
        })
        .expect("validate dual-source isolation");
}

fn import_source(
    case_conn: &rusqlite::Connection,
    active: &app_services::active_case::ActiveCase,
    source_path: &Path,
    platform: DataSourcePlatform,
) -> persistence_sqlite::DbResult<DataSourceId> {
    let before = DataSourceRepo::new(case_conn)
        .find_by_case(&active.meta.id)?
        .into_iter()
        .map(|source| source.id)
        .collect::<Vec<_>>();
    let config = app_services::import_precheck::prepare_import_source_config_from_path(
        &source_path.to_string_lossy(),
        platform,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let job_id = JobRepo::new(case_conn).create(&active.meta.id.0, "dual-source import")?;
    let cancel = Arc::new(AtomicBool::new(false));
    execute_import_job_with_counts(
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
            analysis_mode: ImportAnalysisMode::MetadataOnly,
        },
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

    let created = DataSourceRepo::new(case_conn)
        .find_by_case(&active.meta.id)?
        .into_iter()
        .filter(|source| !before.contains(&source.id))
        .collect::<Vec<_>>();
    assert_eq!(created.len(), 1, "each import must register one source");
    Ok(created[0].id.clone())
}

fn assert_registered_platform(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    expected: &str,
) {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)
        .expect("query source storage")
        .expect("source storage metadata");
    assert_eq!(storage.platform, expected);
    assert_eq!(storage.storage_model, "source_db");
    assert_eq!(storage.import_state, "ready");
}

fn assert_source_db_is_local(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> persistence_sqlite::DbResult<()> {
    let source_conn = app_services::source_db::open_source_db(case_root, data_source_id)?;
    let own_count: i64 = source_conn.query_row(
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
        [&data_source_id.0],
        |row| row.get(0),
    )?;
    let foreign_count: i64 = source_conn.query_row(
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id <> ?1",
        [&data_source_id.0],
        |row| row.get(0),
    )?;
    assert!(own_count > 0, "source.db must contain its own file tree");
    assert_eq!(
        foreign_count, 0,
        "source.db must not contain another source"
    );
    Ok(())
}

fn write_fixture(root: &Path, relative_path: &str, contents: &[u8]) {
    let path = root.join(relative_path);
    std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture path");
    std::fs::write(path, contents).expect("write fixture file");
}
