use app_services::{case_service, file_service, search_service, timeline_service};
use evidence_core::LogicalFsReader;
use persistence_sqlite::repositories::{file_repo::FileRepo, job_repo::JobRepo};
use tempfile::TempDir;
use transport::commands::ExportScopeDto;

#[test]
fn full_case_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let evidence_dir = tmp.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(
        evidence_dir.join("readme.txt"),
        b"forensics analysis pipeline test",
    )
    .unwrap();
    std::fs::write(evidence_dir.join("config.json"), b"{\"key\": \"value\"}").unwrap();

    let cases_dir = tmp.path().join("cases");
    let active = case_service::create_case(&cases_dir, "integration-test", Some("tester")).unwrap();
    let case_id = active.meta.id.clone();

    let index_dir = active.case_root.join("indexes").join("tantivy");

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());

            // Insert data source record to satisfy FK
            persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "test-evidence".into(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Import
            let fs = LogicalFsReader::open(&evidence_dir, "test-evidence")
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let stats = file_service::enumerate_filesystem(conn, &ds_id, &fs)?;
            assert!(
                stats.file_count >= 2,
                "Expected at least 2 files, got {}",
                stats.file_count
            );

            // Read files for timeline
            let repo = FileRepo::new(conn);
            let roots = repo.find_roots(&ds_id)?;
            let mut all_files = Vec::new();
            let mut queue = roots;
            while let Some(f) = queue.pop() {
                if f.entry_type != domain::EntryType::Directory {
                    all_files.push(f);
                } else {
                    queue.extend(repo.find_children(&f.id)?);
                }
            }
            assert!(!all_files.is_empty());

            // Timeline
            let tl_count = timeline_service::project_and_store_file_activity(conn, &all_files)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(tl_count > 0, "Expected timeline events");

            // Search indexing: build a map of entry paths, then provide readers
            let path_map: std::collections::HashMap<String, String> = all_files
                .iter()
                .map(|f| (f.id.0.clone(), f.path.clone()))
                .collect();
            let ev = evidence_dir.clone();
            let to_index: Vec<domain::FileEntryId> =
                all_files.iter().take(10).map(|f| f.id.clone()).collect();
            let idx_result =
                search_service::index_files(conn, &index_dir, &to_index, move |file_id| {
                    let rel_path = path_map.get(&file_id.0)?;
                    let abs_path = ev.join(if rel_path.is_empty() { "." } else { rel_path });
                    std::fs::File::open(&abs_path)
                        .ok()
                        .map(|r| Box::new(r) as Box<dyn std::io::Read>)
                })
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(idx_result.indexed_count > 0, "Expected indexed files");

            // Search query
            let results = search_service::search_files_real(&index_dir, "forensics", 0, 50)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(results.total > 0, "Expected search results for 'forensics'");
            assert!(!results.items.is_empty());
            assert!(!results.items[0].snippets.is_empty());

            // Report
            let output_dir = active.case_root.join("reports");
            let report_file = app_services::report::generate_html_report(
                conn,
                &active.meta,
                &output_dir,
                &ExportScopeDto::default(),
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(output_dir.join(&report_file).exists());

            // Job tracking
            let job_id = JobRepo::new(conn).create(&case_id.0, "test_job")?;
            JobRepo::new(conn).update_progress(&job_id, 50, "halfway")?;
            JobRepo::new(conn).complete(&job_id, "done")?;

            Ok(())
        })
        .unwrap();
}
