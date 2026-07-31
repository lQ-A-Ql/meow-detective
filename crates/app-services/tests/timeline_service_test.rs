//! Integration tests for timeline_service.

use app_services::{case_service, file_service, timeline_service};
use evidence_core::LogicalFsReader;
use tempfile::TempDir;

fn setup_test_case() -> (TempDir, app_services::active_case::ActiveCase) {
    let tmp = TempDir::new().unwrap();
    let cases_dir = tmp.path().join("cases");
    let active = case_service::create_case(&cases_dir, "timeline-test", Some("tester")).unwrap();
    (tmp, active)
}

fn create_test_files(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("file1.txt"), b"content1").unwrap();
    std::fs::write(dir.join("file2.txt"), b"content2").unwrap();
    std::fs::write(dir.join("file3.log"), b"log content").unwrap();
}

#[test]
fn project_and_query_timeline() {
    let (tmp, active) = setup_test_case();
    let evidence_dir = tmp.path().join("evidence");
    create_test_files(&evidence_dir);

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());

            // Insert data source
            persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn).insert(
                &active.meta.id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "test-evidence".into(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Enumerate files
            let fs = LogicalFsReader::open(&evidence_dir, "test-evidence")
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            file_service::enumerate_filesystem(conn, &ds_id, &fs)?;

            // Get all files
            let repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
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
            assert!(!all_files.is_empty(), "Expected files");

            // Project timeline
            let tl_count = timeline_service::project_and_store_file_activity(conn, &all_files)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(tl_count > 0, "Expected timeline events");

            // Query timeline
            let result = timeline_service::query_timeline(conn, 0, 100)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(result.total > 0, "Expected total > 0");
            assert!(!result.items.is_empty(), "Expected items");

            // Verify event structure
            let first = &result.items[0];
            assert!(!first.id.is_empty(), "Expected non-empty id");
            assert!(!first.ts.is_empty(), "Expected non-empty timestamp");
            assert!(
                first.event_type.starts_with("FILE_"),
                "Expected FILE_ event type"
            );

            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_pagination() {
    let (tmp, active) = setup_test_case();
    let evidence_dir = tmp.path().join("evidence");
    create_test_files(&evidence_dir);

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());

            persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn).insert(
                &active.meta.id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "test-evidence".into(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let fs = LogicalFsReader::open(&evidence_dir, "test-evidence")
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            file_service::enumerate_filesystem(conn, &ds_id, &fs)?;

            let repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
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

            let tl_count = timeline_service::project_and_store_file_activity(conn, &all_files)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            // Query with limit
            let result1 = timeline_service::query_timeline(conn, 0, 2)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(result1.items.len(), 2.min(tl_count as usize));
            assert_eq!(result1.total, tl_count);

            // Query with offset
            let result2 = timeline_service::query_timeline(conn, 2, 100)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(result2.total, tl_count);
            // Items may be empty if total <= 2

            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_filtered_by_event_type() {
    let (tmp, active) = setup_test_case();
    let evidence_dir = tmp.path().join("evidence");
    create_test_files(&evidence_dir);

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());

            persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn).insert(
                &active.meta.id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "test-evidence".into(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let fs = LogicalFsReader::open(&evidence_dir, "test-evidence")
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            file_service::enumerate_filesystem(conn, &ds_id, &fs)?;

            let repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
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

            timeline_service::project_and_store_file_activity(conn, &all_files)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            // Query all events
            let all = timeline_service::query_timeline(conn, 0, 1000)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            // Query filtered by FILE_CREATED
            let filtered = timeline_service::query_timeline_filtered(
                conn,
                0,
                1000,
                None,
                None,
                Some("FILE_CREATED"),
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            // Should have fewer or equal events
            assert!(
                filtered.total <= all.total,
                "Filtered total should be <= all total"
            );

            // All filtered events should be FILE_CREATED
            for event in &filtered.items {
                assert_eq!(event.event_type, "FILE_CREATED");
            }

            Ok(())
        })
        .unwrap();
}
