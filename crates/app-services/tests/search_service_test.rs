//! Integration tests for search_service.

use app_services::{case_service, file_service, search_service};
use evidence_core::LogicalFsReader;
use std::collections::HashMap;
use tempfile::TempDir;

fn setup_test_case() -> (TempDir, app_services::active_case::ActiveCase) {
    let tmp = TempDir::new().unwrap();
    let cases_dir = tmp.path().join("cases");
    let active = case_service::create_case(&cases_dir, "search-test", Some("tester")).unwrap();
    (tmp, active)
}

fn create_test_files(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("readme.txt"), b"forensics analysis test document").unwrap();
    std::fs::write(dir.join("config.json"), b"{\"key\": \"forensics value\"}").unwrap();
    std::fs::write(dir.join("notes.md"), b"# Investigation notes\nFind evidence here").unwrap();
    std::fs::write(dir.join("binary.dat"), vec![0u8; 100]).unwrap(); // Non-text file
}

#[test]
fn index_and_search_basic() {
    let (tmp, active) = setup_test_case();
    let evidence_dir = tmp.path().join("evidence");
    create_test_files(&evidence_dir);

    let index_dir = active.case_root.join("indexes").join("tantivy");

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
                },
            )?;

            // Enumerate files
            let fs = LogicalFsReader::open(&evidence_dir, "test-evidence")
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let stats = file_service::enumerate_filesystem(conn, &ds_id, &fs)?;
            assert!(stats.file_count >= 3, "Expected at least 3 files");

            // Get files for indexing
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

            // Build path map and index
            let path_map: HashMap<String, String> = all_files
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
                .map_err(persistence_sqlite::DbError::System)?;

            assert!(idx_result.indexed_count > 0, "Expected indexed files");

            // Search for "forensics"
            let results = search_service::search_files_real(&index_dir, "forensics", 0, 50)
                .map_err(persistence_sqlite::DbError::System)?;
            assert!(results.total > 0, "Expected search results");
            assert!(!results.items.is_empty());

            // Verify snippets
            let first_hit = &results.items[0];
            assert!(!first_hit.snippets.is_empty(), "Expected snippets");
            assert!(
                first_hit.snippets[0].text.contains("forensics")
                    || first_hit.snippets[0]
                        .highlights
                        .iter()
                        .any(|h| h.start < h.end),
                "Expected 'forensics' in snippet or valid highlights"
            );

            Ok(())
        })
        .unwrap();
}

#[test]
fn search_with_pagination() {
    let (tmp, active) = setup_test_case();
    let evidence_dir = tmp.path().join("evidence");
    create_test_files(&evidence_dir);

    let index_dir = active.case_root.join("indexes").join("tantivy");

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

            let path_map: HashMap<String, String> = all_files
                .iter()
                .map(|f| (f.id.0.clone(), f.path.clone()))
                .collect();
            let ev = evidence_dir.clone();
            let to_index: Vec<domain::FileEntryId> =
                all_files.iter().take(10).map(|f| f.id.clone()).collect();

            search_service::index_files(conn, &index_dir, &to_index, move |file_id| {
                let rel_path = path_map.get(&file_id.0)?;
                let abs_path = ev.join(if rel_path.is_empty() { "." } else { rel_path });
                std::fs::File::open(&abs_path)
                    .ok()
                    .map(|r| Box::new(r) as Box<dyn std::io::Read>)
            })
            .map_err(persistence_sqlite::DbError::System)?;

            // Search with limit
            let results = search_service::search_files_real(&index_dir, "forensics", 0, 1)
                .map_err(persistence_sqlite::DbError::System)?;
            assert!(results.total > 0, "Expected total > 0");
            assert!(results.items.len() <= 1, "Expected at most 1 item");

            // Search with offset
            let results2 = search_service::search_files_real(&index_dir, "forensics", 1, 50)
                .map_err(persistence_sqlite::DbError::System)?;
            // Items may be empty if total is 1
            assert!(results2.total == results.total, "Total should be same");

            Ok(())
        })
        .unwrap();
}

#[test]
fn search_no_results() {
    let (tmp, active) = setup_test_case();
    let evidence_dir = tmp.path().join("evidence");
    create_test_files(&evidence_dir);

    let index_dir = active.case_root.join("indexes").join("tantivy");

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

            let path_map: HashMap<String, String> = all_files
                .iter()
                .map(|f| (f.id.0.clone(), f.path.clone()))
                .collect();
            let ev = evidence_dir.clone();
            let to_index: Vec<domain::FileEntryId> =
                all_files.iter().take(10).map(|f| f.id.clone()).collect();

            search_service::index_files(conn, &index_dir, &to_index, move |file_id| {
                let rel_path = path_map.get(&file_id.0)?;
                let abs_path = ev.join(if rel_path.is_empty() { "." } else { rel_path });
                std::fs::File::open(&abs_path)
                    .ok()
                    .map(|r| Box::new(r) as Box<dyn std::io::Read>)
            })
            .map_err(persistence_sqlite::DbError::System)?;

            // Search for non-existent term
            let results =
                search_service::search_files_real(&index_dir, "nonexistent_xyz_12345", 0, 50)
                    .map_err(persistence_sqlite::DbError::System)?;
            assert_eq!(results.total, 0, "Expected 0 results");
            assert!(results.items.is_empty(), "Expected empty items");

            Ok(())
        })
        .unwrap();
}
