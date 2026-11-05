mod staging_support;

use app_services::staging::{
    get_staging_meta, merge_all_staging_to_main, merge_all_staging_to_main_with_cancel,
    merge_all_staging_to_main_with_catalog, merge_all_staging_to_main_with_stats,
    merge_analysis_staging_to_main, merge_analysis_staging_to_main_with_fjall,
    open_analysis_staging, open_partition_staging, set_staging_meta, set_worker_meta, StagingError,
    StagingManifest,
};
use rusqlite::params;
use staging_support::{
    attached_db_names, create_main_analysis_tables, create_main_file_entries_table, done_partition,
};

#[test]
fn staging_merge_honors_cancellation_before_writing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let mut manifest = StagingManifest::create("ds-cancel", "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 1));
    let cancelled = std::sync::atomic::AtomicBool::new(true);

    let error = match merge_all_staging_to_main_with_cancel(
        &main,
        tmp.path(),
        "ds-cancel",
        &manifest,
        None,
        Some(&cancelled),
    ) {
        Ok(_) => panic!("cancelled merge unexpectedly succeeded"),
        Err(error) => error,
    };

    assert!(matches!(error, StagingError::Cancelled));
    let total: i64 = main
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(total, 0);
}

#[test]
fn staging_merge_combines_two_partitions_and_reports_progress() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-two-part";

    for (partition, rows) in [(0, 3), (1, 2)] {
        let staging = open_partition_staging(tmp.path(), ds_id, partition).unwrap();
        for row in 0..rows {
            staging
                .execute(
                    "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                     VALUES (?1, ?2, ?3, ?4, 'File')",
                    params![
                        format!("p{partition}f{row}"),
                        ds_id,
                        format!("/p{partition}/file{row}.txt"),
                        format!("file{row}.txt")
                    ],
                )
                .unwrap();
        }
    }

    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 3));
    manifest.partitions.push(done_partition(1, "P1", 2));
    let progress = std::sync::Mutex::new(Vec::new());
    let callback = |completed, total| progress.lock().unwrap().push((completed, total));

    let stats =
        merge_all_staging_to_main_with_stats(&main, tmp.path(), ds_id, &manifest, Some(&callback))
            .unwrap();

    assert_eq!(stats.staging_rows, 5);
    assert_eq!(stats.merged_rows, 5);
    assert_eq!(stats.ignored_rows, 0);
    assert_eq!(*progress.lock().unwrap(), vec![(1, 2), (2, 2)]);
    let total: i64 = main
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(total, 7);
    let missing_partition_index: i64 = main
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE partition_index IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let unexpected_partition_index: i64 = main
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE partition_index NOT IN (0, 1)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(missing_partition_index, 0);
    assert_eq!(unexpected_partition_index, 0);
    let unknown_encryption_rows: i64 = main
        .query_row(
            "SELECT COUNT(*) FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE AND encrypted IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unknown_encryption_rows, 5);
}

#[test]
fn staging_merge_projects_fjall_before_committing_sqlite_projection() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&main).unwrap();
    let ds_id = "ds-fjall-first";
    main.execute(
        "INSERT INTO data_sources
         (id, case_id, name, kind, source_path, imported_at)
         VALUES (?1, 'case-1', 'source', 'e01', 'source.E01', datetime('now'))",
        [ds_id],
    )
    .unwrap();
    let fjall = app_services::source_db::open_fjall_source_read_write(tmp.path(), ds_id).unwrap();
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    staging
        .execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES ('f1', ?1, '/f1', 'f1', 'File')",
            [ds_id],
        )
        .unwrap();
    drop(staging);
    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 1));

    merge_all_staging_to_main_with_catalog(&main, tmp.path(), ds_id, &manifest, &fjall, None, None)
        .unwrap();

    let sqlite_count: i64 = main
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
            [ds_id],
            |row| row.get(0),
        )
        .unwrap();
    let fjall_page = fjall
        .catalog_list_all(
            &fjall.snapshot(),
            ds_id,
            None,
            persistence_fjall::ScanDirection::Forward,
            32,
        )
        .unwrap();
    assert_eq!(sqlite_count, 2);
    assert_eq!(fjall_page.records.len(), 2);
    assert!(fjall_page.records.iter().any(|record| record.id == "f1"));
}

#[test]
fn staging_merge_preserves_encrypted_file_flag() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-encrypted";
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    staging
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, read_only, encrypted)
             VALUES ('efs-file', ?1, '/secret.txt', 'secret.txt', 'file', 1, 1)",
            [ds_id],
        )
        .unwrap();
    drop(staging);
    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "NTFS", 1));

    merge_all_staging_to_main(&main, tmp.path(), ds_id, &manifest, None).unwrap();

    let (read_only, encrypted): (bool, bool) = main
        .query_row(
            "SELECT read_only <> 0, encrypted <> 0
             FROM file_entries WHERE id = 'efs-file'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(read_only);
    assert!(encrypted);
}

#[test]
fn staging_merge_is_idempotent_after_merged_marker() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-idempotent";
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    staging
        .execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES ('f1', ?1, '/f1', 'f1', 'File')",
            [ds_id],
        )
        .unwrap();
    drop(staging);
    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 1));

    assert_eq!(
        merge_all_staging_to_main(&main, tmp.path(), ds_id, &manifest, None).unwrap(),
        1
    );
    assert_eq!(
        merge_all_staging_to_main(&main, tmp.path(), ds_id, &manifest, None).unwrap(),
        0
    );
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    assert_eq!(
        get_staging_meta(&staging, "merged").unwrap().as_deref(),
        Some("true")
    );
}

#[test]
fn staging_merge_conflict_is_visible_and_rolls_back() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-conflict";
    main.execute(
        "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
         VALUES ('existing', ?1, '/existing', 'existing', 'file')",
        [ds_id],
    )
    .unwrap();
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    staging
        .execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES ('existing', ?1, '/changed', 'changed', 'File')",
            [ds_id],
        )
        .unwrap();
    drop(staging);
    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 1));

    let error = merge_all_staging_to_main_with_stats(&main, tmp.path(), ds_id, &manifest, None)
        .unwrap_err();
    assert!(error.to_string().contains("Merge partition 0"));
    let path: String = main
        .query_row(
            "SELECT path FROM file_entries WHERE id = 'existing'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(path, "/existing");
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    assert_ne!(
        get_staging_meta(&staging, "merged").unwrap().as_deref(),
        Some("true")
    );
}

#[test]
fn staging_merge_failure_rolls_back_and_detaches_database() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::migrations::runner::run_all(&main).unwrap();
    let ds_id = "ds-failure";
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    staging
        .execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES ('f1', ?1, '/f1', 'f1', 'File')",
            [ds_id],
        )
        .unwrap();
    drop(staging);
    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 1));

    let error = merge_all_staging_to_main(&main, tmp.path(), ds_id, &manifest, None)
        .expect_err("an app-db-shaped target must be rejected before staging attach");
    assert!(
        error
            .to_string()
            .contains("source merge target is not a source database"),
        "unexpected merge target error: {error}"
    );
    let count: i64 = main
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert!(!attached_db_names(&main)
        .iter()
        .any(|name| name == "staging"));
}

#[test]
fn staging_merge_skips_previously_merged_and_empty_databases() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-skip";
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    set_staging_meta(&staging, "merged", "true").unwrap();
    drop(staging);
    let _empty = open_partition_staging(tmp.path(), ds_id, 1).unwrap();
    drop(_empty);
    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 0));
    manifest.partitions.push(done_partition(1, "P1", 0));

    assert_eq!(
        merge_all_staging_to_main(&main, tmp.path(), ds_id, &manifest, None).unwrap(),
        0
    );
}

#[test]
fn staging_analysis_merge_skips_already_merged_worker() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_analysis_tables(&main);
    let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
    worker
        .execute(
            "INSERT INTO artifact_rows
             (id, file_id, artifact_type, display_name, summary, data_json, source_path, created_at)
             VALUES ('a1', 'f1', 'Prefetch', 'Artifact', '', '{}', '', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    set_worker_meta(&worker, "merged", "true").unwrap();
    drop(worker);

    let stats = merge_analysis_staging_to_main(
        &main,
        tmp.path(),
        "ds-analysis",
        &[0],
        "case-1",
        &tmp.path().join("index"),
        None,
    )
    .unwrap();
    assert_eq!(stats.artifact_count, 0);
}

#[test]
fn staging_analysis_projects_fjall_before_sqlite_commit() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&main).unwrap();
    main.execute(
        "INSERT INTO data_sources
         (id, case_id, name, kind, source_path, imported_at)
         VALUES ('ds-analysis-fjall', 'case-1', 'source', 'e01', 'source.E01', datetime('now'))",
        [],
    )
    .unwrap();
    let worker = open_analysis_staging(tmp.path(), "ds-analysis-fjall", 0).unwrap();
    worker
        .execute(
            "INSERT INTO artifact_rows
             (id, file_id, artifact_type, display_name, summary, data_json, source_path, created_at)
             VALUES ('artifact-fjall', 'file-1', 'Test', 'Artifact', '', '{}', '', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    drop(worker);
    let fjall =
        app_services::source_db::open_fjall_source_read_write(tmp.path(), "ds-analysis-fjall")
            .unwrap();

    let stats = merge_analysis_staging_to_main_with_fjall(
        &main,
        tmp.path(),
        "ds-analysis-fjall",
        &[0],
        "case-1",
        &tmp.path().join("index"),
        &fjall,
        None,
    )
    .unwrap();
    assert_eq!(stats.artifact_count, 1);
    assert_eq!(
        main.query_row(
            "SELECT COUNT(*) FROM artifacts WHERE id = 'artifact-fjall'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    drop(fjall);
    let fjall =
        app_services::source_db::open_fjall_source_read_only(tmp.path(), "ds-analysis-fjall")
            .unwrap();
    let snapshot = fjall.snapshot();
    assert!(fjall
        .artifact_get(&snapshot, "ds-analysis-fjall", "artifact-fjall")
        .unwrap()
        .is_some());
}

#[test]
fn staging_analysis_index_merge_is_paged() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_analysis_tables(&main);
    let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
    for index in 0..55 {
        worker
            .execute(
                "INSERT INTO index_docs (file_id, path, text, language, truncated)
                 VALUES (?1, ?2, ?3, 'utf-8', 0)",
                params![
                    format!("f-{index:03}"),
                    format!("file-{index:03}.txt"),
                    format!("marker page-test-{index:03}")
                ],
            )
            .unwrap();
    }
    drop(worker);

    let stats = merge_analysis_staging_to_main(
        &main,
        tmp.path(),
        "ds-analysis",
        &[0],
        "case-1",
        &tmp.path().join("index"),
        None,
    )
    .unwrap();
    assert_eq!(stats.indexed_count, 55);
}

#[test]
fn staging_analysis_merge_failure_rolls_back_and_detaches_database() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    main.execute_batch(
        "CREATE TABLE artifacts (
            id TEXT PRIMARY KEY NOT NULL, case_id TEXT NOT NULL DEFAULT '',
            data_source_id TEXT NOT NULL DEFAULT '', artifact_type TEXT NOT NULL,
            source_object_id TEXT, extractor_id TEXT, extractor_version TEXT,
            confidence REAL, source_attribution TEXT, title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '', attrs TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, title)
        VALUES ('existing', 'case-1', 'ds-analysis', 'x', 'existing');",
    )
    .unwrap();
    let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
    worker
        .execute(
            "INSERT INTO artifact_rows
             (id, file_id, artifact_type, display_name, summary, data_json, source_path, created_at)
             VALUES ('a1', 'f1', 'Prefetch', 'Artifact', '', '{}', '', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    drop(worker);

    assert!(merge_analysis_staging_to_main(
        &main,
        tmp.path(),
        "ds-analysis",
        &[0],
        "case-1",
        &tmp.path().join("index"),
        None,
    )
    .is_err());
    let count: i64 = main
        .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(!attached_db_names(&main)
        .iter()
        .any(|name| name == "analysis_stage"));
}
