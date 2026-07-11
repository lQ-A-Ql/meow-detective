use app_services::{analysis_service, case_service, file_service};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;

#[test]
fn analysis_demo_seed_uses_source_db_and_supports_real_analysis_flow() {
    let temp = tempfile::TempDir::new().expect("create demo parent");
    let active = case_service::create_case(temp.path(), "Analysis Demo", Some("Codex Demo"))
        .expect("create demo case");
    analysis_service::seed_analysis_demo_data(&active).expect("seed demo case");

    active
        .with_conn(|case_conn| {
            let source = DataSourceRepo::new(case_conn)
                .find_by_case(&active.meta.id)?
                .into_iter()
                .next()
                .expect("demo source registration");
            let storage = DataSourceRepo::new(case_conn)
                .find_storage(&source.id)?
                .expect("demo source storage");
            assert_eq!(storage.platform, "windows");
            assert_eq!(storage.import_state, "ready");
            let app_file_count: i64 =
                case_conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
            assert_eq!(app_file_count, 0, "demo file tree must stay in source.db");

            let source_conn = app_services::source_db::open_registered_source_db(
                case_conn,
                &active.case_root,
                &source.id,
            )?;
            let info = analysis_service::extract_system_info_for_case(
                &source_conn,
                |file_id, max_bytes| {
                    file_service::read_file_header_by_id(&source_conn, file_id, max_bytes)
                },
            );
            assert!(matches!(
                info.status,
                transport::dto::AnalysisParseStatusDto::Parsed
                    | transport::dto::AnalysisParseStatusDto::Partial
            ));
            assert!(info.computer_name.is_some());
            assert!(info.os_version.is_some());
            assert!(info
                .provenance
                .iter()
                .any(|item| item.parser == "registry.system"));
            assert!(info
                .provenance
                .iter()
                .any(|item| item.parser == "evtx.boot_shutdown"));

            let files =
                analysis_service::collect_file_entries(&source_conn).expect("collect demo files");
            let classifications =
                analysis_service::classify_files_by_magic(&files, 5000, |file_id| {
                    file_service::read_file_header_by_id(
                        &source_conn,
                        file_id,
                        analysis_service::MAGIC_HEADER_LIMIT,
                    )
                });
            let detected = classifications
                .iter()
                .flat_map(|category| category.files.iter())
                .map(|file| file.file_type.as_str())
                .collect::<Vec<_>>();
            assert!(detected.contains(&"PDF"));
            assert!(detected.contains(&"PE"));
            assert!(detected.contains(&"ZIP"));
            Ok(())
        })
        .expect("verify demo analysis");
}
