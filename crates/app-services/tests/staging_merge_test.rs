mod staging_support;

use app_services::staging::{
    get_staging_meta, merge_all_staging_to_main_with_catalog,
    merge_analysis_staging_to_main_with_fjall, open_analysis_staging, open_partition_staging,
    set_staging_meta, set_worker_meta, StagingError, StagingManifest,
};
use rusqlite::params;
use staging_support::{attached_db_names, done_partition, NativeSourceFixture};

#[test]
fn staging_merge_honors_cancellation_before_writing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = NativeSourceFixture::new(tmp.path(), "ds-cancel");
    let mut manifest = StagingManifest::create("ds-cancel", "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 1));
    let cancelled = std::sync::atomic::AtomicBool::new(true);

    let error = match merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        "ds-cancel",
        &manifest,
        &source.storage,
        None,
        Some(&cancelled),
    ) {
        Ok(_) => panic!("cancelled merge unexpectedly succeeded"),
        Err(error) => error,
    };

    assert!(matches!(error, StagingError::Cancelled));
    assert!(source.catalog_records().is_empty());
}

#[test]
fn staging_merge_combines_two_partitions_and_reports_progress() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-two-part";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);

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

    let stats = merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &manifest,
        &source.storage,
        Some(&callback),
        None,
    )
    .unwrap();

    assert_eq!(stats.staging_rows, 5);
    assert_eq!(stats.merged_rows, 5);
    assert_eq!(stats.ignored_rows, 0);
    assert_eq!(*progress.lock().unwrap(), vec![(1, 2), (2, 2)]);
    let records = source.catalog_records();
    assert_eq!(records.len(), 7);
    let missing_partition_index = records
        .iter()
        .filter(|record| record.partition_index.is_none())
        .count();
    let unexpected_partition_index = records
        .iter()
        .filter(|record| !matches!(record.partition_index, Some(0 | 1)))
        .count();
    assert_eq!(missing_partition_index, 0);
    assert_eq!(unexpected_partition_index, 0);
    let unknown_encryption_rows = records
        .iter()
        .filter(|record| {
            matches!(
                record.entry_type,
                persistence_source::fjall::CatalogEntryType::File
            ) && record.encrypted.is_none()
        })
        .count();
    assert_eq!(unknown_encryption_rows, 5);
}

#[test]
fn staging_merge_projects_fjall_before_committing_sqlite_projection() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-fjall-first";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
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

    merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &manifest,
        &source.storage,
        None,
        None,
    )
    .unwrap();

    let detail_tables: i64 = source
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema
             WHERE type = 'table'
               AND name IN ('file_entries', 'artifacts', 'timeline_events')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(detail_tables, 0);
    let records = source.catalog_records();
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|record| record.id == "f1"));
}

#[test]
fn staging_merge_preserves_encrypted_file_flag() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-encrypted";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
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

    merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &manifest,
        &source.storage,
        None,
        None,
    )
    .unwrap();

    let record = source
        .catalog_records()
        .into_iter()
        .find(|record| record.id == "efs-file")
        .unwrap();
    assert!(record.read_only);
    assert_eq!(record.encrypted, Some(true));
}

#[test]
fn staging_merge_is_idempotent_after_merged_marker() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-idempotent";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
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
        merge_all_staging_to_main_with_catalog(
            &source.conn,
            tmp.path(),
            ds_id,
            &manifest,
            &source.storage,
            None,
            None,
        )
        .unwrap()
        .merged_rows,
        1
    );
    assert_eq!(
        merge_all_staging_to_main_with_catalog(
            &source.conn,
            tmp.path(),
            ds_id,
            &manifest,
            &source.storage,
            None,
            None,
        )
        .unwrap()
        .merged_rows,
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
    let ds_id = "ds-conflict";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
    persistence_source::repositories::file_repo::FileRepo::new(&source.conn)
        .insert_batch(&[domain::FileEntry {
            id: domain::FileEntryId("existing".into()),
            parent_id: None,
            data_source_id: domain::DataSourceId(ds_id.into()),
            path: "/existing".into(),
            name: "existing".into(),
            entry_type: domain::EntryType::File,
            size: None,
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
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

    let error = merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &manifest,
        &source.storage,
        None,
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("Merge partition 0"));
    let record = source
        .catalog_records()
        .into_iter()
        .find(|record| record.id == "existing")
        .unwrap();
    assert_eq!(record.path, "/existing");
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    assert_ne!(
        get_staging_meta(&staging, "merged").unwrap().as_deref(),
        Some("true")
    );
}

#[test]
fn staging_merge_failure_rolls_back_and_detaches_database() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::open_or_create(&tmp.path().join("app.db")).unwrap();
    persistence_sqlite::migrations::runner::run_all(&main).unwrap();
    let ds_id = "ds-failure";
    let foreign_source = NativeSourceFixture::new(tmp.path(), "foreign-source");
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

    let error = merge_all_staging_to_main_with_catalog(
        &main,
        tmp.path(),
        ds_id,
        &manifest,
        &foreign_source.storage,
        None,
        None,
    )
    .expect_err("an app-db-shaped target must be rejected before reading staging");
    assert!(
        error.to_string().contains("source identity is missing"),
        "unexpected merge target error: {error}"
    );
    assert!(!attached_db_names(&main)
        .iter()
        .any(|name| name == "staging"));
}

#[test]
fn staging_merge_skips_previously_merged_and_empty_databases() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-skip";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    set_staging_meta(&staging, "merged", "true").unwrap();
    drop(staging);
    let _empty = open_partition_staging(tmp.path(), ds_id, 1).unwrap();
    drop(_empty);
    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 0));
    manifest.partitions.push(done_partition(1, "P1", 0));

    assert_eq!(
        merge_all_staging_to_main_with_catalog(
            &source.conn,
            tmp.path(),
            ds_id,
            &manifest,
            &source.storage,
            None,
            None,
        )
        .unwrap()
        .merged_rows,
        0
    );
}

#[test]
fn staging_analysis_merge_skips_already_merged_worker() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = NativeSourceFixture::new(tmp.path(), "ds-analysis");
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

    let stats = merge_analysis_staging_to_main_with_fjall(
        &source.conn,
        tmp.path(),
        "ds-analysis",
        &[0],
        "case-1",
        &tmp.path().join("index"),
        &source.storage,
        None,
    )
    .unwrap();
    assert_eq!(stats.artifact_count, 0);
}

#[test]
fn staging_analysis_projects_fjall_before_sqlite_commit() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = NativeSourceFixture::new(tmp.path(), "ds-analysis-fjall");
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

    let stats = merge_analysis_staging_to_main_with_fjall(
        &source.conn,
        tmp.path(),
        "ds-analysis-fjall",
        &[0],
        "case-1",
        &tmp.path().join("index"),
        &source.storage,
        None,
    )
    .unwrap();
    assert_eq!(stats.artifact_count, 1);
    let detail_tables: i64 = source
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema
             WHERE type = 'table'
               AND name IN ('file_entries', 'artifacts', 'timeline_events')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(detail_tables, 0);
    let store = persistence_source::detail::open_store(&source.conn)
        .unwrap()
        .unwrap();
    let artifacts =
        persistence_source::fjall::ArtifactRecordStore::from_source_store((*store).clone());
    let snapshot = artifacts.read_snapshot();
    assert!(artifacts
        .get(&snapshot, "ds-analysis-fjall", "artifact-fjall")
        .unwrap()
        .is_some());
}

#[test]
fn staging_analysis_index_merge_is_paged() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = NativeSourceFixture::new(tmp.path(), "ds-analysis");
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

    let stats = merge_analysis_staging_to_main_with_fjall(
        &source.conn,
        tmp.path(),
        "ds-analysis",
        &[0],
        "case-1",
        &tmp.path().join("index"),
        &source.storage,
        None,
    )
    .unwrap();
    assert_eq!(stats.indexed_count, 55);
}

#[test]
fn staging_analysis_merge_failure_rolls_back_and_detaches_database() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = NativeSourceFixture::new(tmp.path(), "ds-analysis");
    let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
    worker
        .execute(
            "INSERT INTO artifact_rows
             (id, file_id, artifact_type, display_name, summary, data_json, source_path, created_at)
             VALUES ('a1', 'f1', 'Prefetch', 'Artifact', '', '{}', '', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    worker
        .execute(
            "INSERT INTO timeline_rows
             (id, file_id, timestamp, event_type, title)
             VALUES ('bad-event', 'f1', 'not-a-timestamp', 'Test', 'Bad')",
            [],
        )
        .unwrap();
    drop(worker);

    assert!(merge_analysis_staging_to_main_with_fjall(
        &source.conn,
        tmp.path(),
        "ds-analysis",
        &[0],
        "case-1",
        &tmp.path().join("index"),
        &source.storage,
        None,
    )
    .is_err());
    let store = persistence_source::detail::open_store(&source.conn)
        .unwrap()
        .unwrap();
    let artifacts =
        persistence_source::fjall::ArtifactRecordStore::from_source_store((*store).clone());
    let snapshot = artifacts.read_snapshot();
    assert!(artifacts
        .get(&snapshot, "ds-analysis", "a1")
        .unwrap()
        .is_none());
    assert!(!attached_db_names(&source.conn)
        .iter()
        .any(|name| name == "analysis_stage"));
}
