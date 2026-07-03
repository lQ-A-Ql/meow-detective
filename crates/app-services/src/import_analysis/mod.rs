//! Post-import analysis worker pool.
//!
//! Workers read file rows from the main DB, write artifacts/timeline/index docs
//! to per-worker temp DBs, then the caller merges those temp DBs with one writer.

mod budget;
pub mod error;
mod finalize;
mod options;
pub mod priority_queue;
mod progress;
mod task_feed;
pub mod tier;
mod worker_pool;
mod worker_runtime;

pub use error::ImportAnalysisError;

pub use budget::{
    content_budget_for_mode, default_memory_hard_limit_mb, default_memory_soft_limit_mb,
    ContentBudget,
};
pub use options::{
    AnalysisProgressCallback, ImportAnalysisMode, ImportAnalysisOptions, ImportAnalysisStats,
    JobOutcomeCounts, PostImportPipelineError, PostImportPipelineOptions,
};
pub use progress::current_rss_mb;
pub use worker_pool::{
    default_analysis_worker_count, resolve_analysis_worker_count, run_import_analysis_staging,
    run_post_import_pipeline_with_counts,
};

#[cfg(test)]
mod tests {
    use super::tier::TierStateMachine;
    use super::*;
    use super::{
        finalize::{prepare_analysis_staging_startup, AnalysisStartupAction},
        progress::set_test_rss_override_mb,
        task_feed::{
            analysis_task_queue_bound, count_analysis_file_tasks, fetch_analysis_file_page,
        },
        worker_runtime::{
            reserve_content_budget, should_extract_artifact, should_index_file, test_hooks,
            SharedAnalysisState,
        },
    };
    use crate::{artifact_service, staging};
    use chrono::{TimeZone, Utc};
    use domain::{
        CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType,
        FileEntry, FileEntryId,
    };
    use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
    use persistence_sqlite::runner;
    use rusqlite::{params, Connection};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    static TEST_HOOK_LOCK: Mutex<()> = Mutex::new(());

    fn setup_case_db(tmp: &TempDir) -> (PathBuf, DataSourceId) {
        let db_path = tmp.path().join("app.db");
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-1', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
             VALUES ('ds-1', 'case-1', 'logical', 'logical_directory', ?1, '2026-01-01T00:00:00Z')",
            params![tmp.path().join("evidence").display().to_string()],
        )
        .unwrap();
        (db_path, DataSourceId("ds-1".to_string()))
    }

    fn insert_file_with_type(
        conn: &Connection,
        id: &str,
        ds: &DataSourceId,
        path: &str,
        entry_type: &str,
    ) {
        conn.execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 12, 'txt', 0, ?6)",
            params![
                id,
                ds.0,
                path,
                path.rsplit('/').next().unwrap_or(path),
                entry_type,
                Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0)
                    .unwrap()
                    .to_rfc3339()
            ],
        )
        .unwrap();
    }

    fn insert_file(conn: &Connection, id: &str, ds: &DataSourceId, path: &str) {
        insert_file_with_type(conn, id, ds, path, "file");
    }

    fn insert_staged_index_doc(conn: &Connection, file_id: &str, text: &str) {
        conn.execute(
            "INSERT OR REPLACE INTO index_docs
             (file_id, path, text, language, truncated)
             VALUES (?1, ?2, ?3, 'unknown', 0)",
            params![file_id, format!("{file_id}.txt"), text],
        )
        .unwrap();
    }

    fn write_exfat_text_raw_fixture(path: &std::path::Path) -> std::io::Result<()> {
        write_exfat_single_file_raw_fixture(path, "LARGE.TXT", &[b'A'; 1536])
    }

    fn write_exfat_single_file_raw_fixture(
        path: &std::path::Path,
        file_name: &str,
        content: &[u8],
    ) -> std::io::Result<()> {
        const SECTOR_SIZE: usize = 512;
        const FAT_SECTOR: usize = 24;
        const CLUSTER_HEAP_SECTOR: usize = 32;
        const CLUSTER_SIZE: usize = SECTOR_SIZE;
        const TOTAL_SECTORS: usize = 1024;

        let mut data = vec![0u8; TOTAL_SECTORS * SECTOR_SIZE];
        let file_size = content.len();
        let file_clusters = file_size.div_ceil(CLUSTER_SIZE).max(1);

        let boot = &mut data[0..SECTOR_SIZE];
        boot[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
        boot[3..11].copy_from_slice(b"EXFAT   ");
        boot[72..80].copy_from_slice(&(TOTAL_SECTORS as u64).to_le_bytes());
        boot[80..84].copy_from_slice(&(FAT_SECTOR as u32).to_le_bytes());
        boot[84..88].copy_from_slice(&1u32.to_le_bytes());
        boot[88..92].copy_from_slice(&(CLUSTER_HEAP_SECTOR as u32).to_le_bytes());
        boot[92..96].copy_from_slice(&100u32.to_le_bytes());
        boot[96..100].copy_from_slice(&2u32.to_le_bytes());
        boot[100..104].copy_from_slice(&0x12345678u32.to_le_bytes());
        boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
        boot[108] = 9;
        boot[109] = 0;
        boot[110] = 1;
        boot[111] = 0x80;
        boot[112] = 0xFF;
        boot[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());

        let fat_offset = FAT_SECTOR * SECTOR_SIZE;
        let fat = &mut data[fat_offset..fat_offset + SECTOR_SIZE];
        fat[0..4].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF]);
        fat[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        fat[8..12].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        for cluster in 3..3 + file_clusters {
            let offset = cluster * 4;
            fat[offset..offset + 4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        }

        let root_offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE;
        let root = &mut data[root_offset..root_offset + CLUSTER_SIZE];
        let mut pos = 0usize;

        root[pos] = 0x85;
        root[pos + 1] = 0x02;
        root[pos + 4..pos + 6].copy_from_slice(&0x20u16.to_le_bytes());
        pos += 32;

        root[pos] = 0xC0;
        root[pos + 1] = 0x02;
        root[pos + 3] = file_name.encode_utf16().count() as u8;
        root[pos + 8..pos + 16].copy_from_slice(&(file_size as u64).to_le_bytes());
        root[pos + 20..pos + 24].copy_from_slice(&3u32.to_le_bytes());
        root[pos + 24..pos + 32].copy_from_slice(&(file_size as u64).to_le_bytes());
        pos += 32;

        root[pos] = 0xC1;
        for (i, ch) in file_name.encode_utf16().enumerate() {
            let offset = pos + 2 + i * 2;
            root[offset..offset + 2].copy_from_slice(&ch.to_le_bytes());
        }

        let file_offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE + CLUSTER_SIZE;
        data[file_offset..file_offset + content.len()].copy_from_slice(content);

        std::fs::write(path, data)
    }

    fn recycle_bin_i_file_bytes(original_path: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&24u64.to_le_bytes());
        bytes.extend_from_slice(&4096u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        for ch in original_path.encode_utf16() {
            bytes.extend_from_slice(&ch.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes
    }

    fn set_done_worker_meta(
        conn: &Connection,
        worker_count: usize,
        merged: bool,
        processed_count: u64,
    ) {
        staging::set_worker_meta(conn, "status", "done").unwrap();
        staging::set_worker_meta(conn, "merged", if merged { "true" } else { "false" }).unwrap();
        staging::set_worker_meta(conn, "worker_count", &worker_count.to_string()).unwrap();
        staging::set_worker_meta(conn, "processed_count", &processed_count.to_string()).unwrap();
    }

    fn analysis_options(
        tmp: &TempDir,
        db_path: PathBuf,
        ds_id: DataSourceId,
        mode: ImportAnalysisMode,
    ) -> ImportAnalysisOptions {
        ImportAnalysisOptions {
            case_root: tmp.path().to_path_buf(),
            db_path,
            case_id: "case-1".to_string(),
            data_source_id: ds_id,
            index_dir: tmp.path().join("indexes").join("tantivy"),
            max_analysis_workers: Some(1),
            cancel_token: Arc::new(AtomicBool::new(false)),
            enable_timeline_projection: true,
            enable_content_extraction: mode.allows_content(),
            enable_text_indexing: mode.allows_content(),
            analysis_mode: mode,
            content_budget: content_budget_for_mode(mode),
            memory_soft_limit_mb: default_memory_soft_limit_mb(),
            memory_hard_limit_mb: default_memory_hard_limit_mb(),
            tier_state: Arc::new(Mutex::new(TierStateMachine::new())),
        }
    }

    fn post_import_options(
        tmp: &TempDir,
        db_path: PathBuf,
        ds_id: DataSourceId,
        mode: ImportAnalysisMode,
    ) -> PostImportPipelineOptions {
        PostImportPipelineOptions {
            case_root: tmp.path().to_path_buf(),
            db_path,
            case_id: "case-1".to_string(),
            data_source_id: ds_id,
            index_dir: tmp.path().join("indexes").join("tantivy"),
            max_analysis_workers: Some(1),
            cancel_token: Arc::new(AtomicBool::new(false)),
            enable_timeline_projection: true,
            enable_content_extraction: mode.allows_content(),
            enable_text_indexing: mode.allows_content(),
            analysis_mode: mode,
            tier_state: Arc::new(Mutex::new(TierStateMachine::new())),
        }
    }

    #[test]
    fn post_import_skip_uses_progress_sink_without_running_workers() {
        let tmp = TempDir::new().unwrap();
        let options = PostImportPipelineOptions {
            case_root: tmp.path().to_path_buf(),
            db_path: tmp.path().join("app.db"),
            case_id: "case-1".to_string(),
            data_source_id: DataSourceId("ds-1".to_string()),
            index_dir: tmp.path().join("indexes").join("tantivy"),
            max_analysis_workers: Some(1),
            cancel_token: Arc::new(AtomicBool::new(false)),
            enable_timeline_projection: false,
            enable_content_extraction: false,
            enable_text_indexing: false,
            analysis_mode: ImportAnalysisMode::MetadataOnly,
            tier_state: Arc::new(Mutex::new(TierStateMachine::new())),
        };
        let events = std::sync::Mutex::new(Vec::new());
        let progress = |pct: u32, detail: &str| {
            events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((pct, detail.to_string()));
        };

        let (message, counts) = run_post_import_pipeline_with_counts(options, Some(&progress))
            .expect("post import skip");

        assert_eq!(
            message,
            "Timeline: deferred until Timeline page. Artifacts: 0. Index: 0 indexed"
        );
        assert_eq!(counts, JobOutcomeCounts::default());
        let events = events.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, 84);
        assert!(events[0].1.contains("phase=post-import-skip"));
        assert!(events[0].1.contains("scheduling=deferred"));
        assert!(events[0].1.contains("workerBudget=1"));
        assert!(events[0].1.contains("activeWorkers=0"));
        assert!(events[0].1.contains("queuedTasks=0"));
        assert!(events[0].1.contains("pendingTasks=0"));
        assert!(events[0].1.contains("contentDeferred=true"));
        assert!(events[0].1.contains("textDeferred=true"));
    }

    #[test]
    fn post_import_worker_staging_success_preserves_summary_and_counts() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        insert_file(&conn, "f-b", &ds_id, "b.txt");
        drop(conn);
        let options = post_import_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        let events = std::sync::Mutex::new(Vec::new());
        let progress = |pct: u32, detail: &str| {
            events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((pct, detail.to_string()));
        };

        let (message, counts) = run_post_import_pipeline_with_counts(options, Some(&progress))
            .expect("post import success");

        assert!(message.starts_with("Timeline: 2 events"));
        assert!(message.contains("Artifacts: 0. Index: 0 indexed"));
        assert_eq!(counts, JobOutcomeCounts::default());
        let events = events.lock().unwrap_or_else(|e| e.into_inner());
        let scheduled = events
            .iter()
            .find(|(_, detail)| detail.contains("Post-import analysis scheduled"))
            .expect("scheduled progress");
        assert!(scheduled.1.contains("scheduling=queued"));
        assert!(scheduled.1.contains("workerBudget=1"));
        assert!(scheduled.1.contains("contentDeferred=true"));
        assert!(scheduled.1.contains("textDeferred=true"));
        let started = events
            .iter()
            .find(|(_, detail)| detail.contains("Analysis staging:"))
            .expect("analysis start progress");
        assert!(started.1.contains("scheduling=queued"));
        assert!(started.1.contains("queueBound=256"));
        assert!(started.1.contains("pendingTasks=2"));
        assert!(events
            .iter()
            .any(|(_, detail)| detail.contains("Analysis workers complete")
                && detail.contains("workerBudget=1")
                && detail.contains("pendingTasks=0")));
        let main_conn = persistence_sqlite::open_or_create(&tmp.path().join("app.db")).unwrap();
        let timeline_count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeline_count, 2);
    }

    #[test]
    fn post_import_cancel_failure_preserves_partial_counts() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        drop(conn);
        let cancel = Arc::new(AtomicBool::new(true));
        let mut options = post_import_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        options.cancel_token = cancel;

        let error = run_post_import_pipeline_with_counts(options, None).unwrap_err();

        assert!(error.message.contains("cancelled"));
        assert_eq!(error.counts.warning_count, 1);
        assert_eq!(error.counts.skipped_count, 1);
        assert_eq!(error.counts.failed_count, 0);
        assert!(error.counts.is_partial());
    }

    #[test]
    fn done_merged_analysis_worker_dbs_are_left_untouched() {
        let tmp = TempDir::new().unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        insert_staged_index_doc(&worker_conn, "already-merged", "keep me");
        set_done_worker_meta(&worker_conn, 1, true, 7);
        drop(worker_conn);

        let action =
            prepare_analysis_staging_startup(&options, &[0], 1, None).expect("startup plan");

        assert_eq!(action, AnalysisStartupAction::AlreadyMerged);
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        let (_artifacts, _timeline, index) =
            staging::analysis_staging_counts(&worker_conn).unwrap();
        assert_eq!(index, 1);
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("true")
        );
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "processed_count")
                .unwrap()
                .as_deref(),
            Some("7")
        );
    }

    #[test]
    fn stale_unmerged_worker_layout_is_reinitialized_when_worker_count_changes() {
        let tmp = TempDir::new().unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );

        let worker0 = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        insert_staged_index_doc(&worker0, "old-worker-0", "stale");
        set_done_worker_meta(&worker0, 2, false, 11);
        drop(worker0);

        let worker1 = staging::open_analysis_staging(tmp.path(), &ds_id.0, 1).unwrap();
        insert_staged_index_doc(&worker1, "old-worker-1", "stale");
        set_done_worker_meta(&worker1, 2, false, 13);
        drop(worker1);
        assert!(staging::analysis_staging_db_path(tmp.path(), &ds_id.0, 1).exists());

        let action =
            prepare_analysis_staging_startup(&options, &[0], 1, None).expect("startup plan");

        assert_eq!(action, AnalysisStartupAction::RunWorkers);
        let worker0 = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        let (_artifacts, _timeline, index) = staging::analysis_staging_counts(&worker0).unwrap();
        assert_eq!(index, 0);
        assert_eq!(
            staging::get_worker_meta(&worker0, "status")
                .unwrap()
                .as_deref(),
            Some("pending")
        );
        assert_eq!(
            staging::get_worker_meta(&worker0, "worker_count")
                .unwrap()
                .as_deref(),
            Some("1")
        );
        assert!(!staging::analysis_staging_db_path(tmp.path(), &ds_id.0, 1).exists());
    }

    #[test]
    fn done_unmerged_matching_layout_resumes_with_merge_only() {
        let tmp = TempDir::new().unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        insert_staged_index_doc(&worker_conn, "ready-to-merge", "keep for merge");
        set_done_worker_meta(&worker_conn, 1, false, 5);
        drop(worker_conn);

        let action =
            prepare_analysis_staging_startup(&options, &[0], 1, None).expect("startup plan");

        assert_eq!(action, AnalysisStartupAction::MergeOnly);
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        let (_artifacts, _timeline, index) =
            staging::analysis_staging_counts(&worker_conn).unwrap();
        assert_eq!(index, 1);
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("false")
        );
    }

    #[test]
    fn analysis_worker_staging_open_creates_expected_tables() {
        let tmp = TempDir::new().unwrap();
        let conn = staging::open_analysis_staging(tmp.path(), "ds-1", 0).unwrap();
        for table in [
            "artifact_rows",
            "timeline_rows",
            "index_docs",
            "worker_meta",
        ] {
            let name: String = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
                    params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(name, table);
        }
    }

    #[test]
    fn analysis_pool_respects_worker_limit_and_writes_temp_db() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        std::fs::write(tmp.path().join("evidence").join("a.txt"), "marker").unwrap();
        std::fs::write(tmp.path().join("evidence").join("b.txt"), "marker").unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        insert_file(&conn, "f-b", &ds_id, "b.txt");
        drop(conn);

        let cancel = Arc::new(AtomicBool::new(false));
        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::BudgetedContent,
        );
        options.cancel_token = cancel;
        let stats = run_import_analysis_staging(options, None).unwrap();

        assert_eq!(stats.worker_ids, vec![0]);
        assert_eq!(stats.processed_count, 2);
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        let (_artifacts, timeline, index) = staging::analysis_staging_counts(&worker_conn).unwrap();
        assert!(timeline > 0);
        assert!(index > 0);
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("true")
        );
    }

    #[test]
    fn analysis_worker_writes_only_own_temp_db() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        std::fs::write(tmp.path().join("evidence").join("a.txt"), "alpha").unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        drop(conn);

        let cancel = Arc::new(AtomicBool::new(false));
        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::BudgetedContent,
        );
        options.cancel_token = cancel;
        let stats = run_import_analysis_staging(options, None).unwrap();

        assert_eq!(stats.worker_ids, vec![0]);
        assert!(staging::analysis_staging_db_path(tmp.path(), &ds_id.0, 0).exists());
        assert!(!staging::analysis_staging_db_path(tmp.path(), &ds_id.0, 1).exists());
    }

    #[test]
    fn analysis_tasks_include_title_case_file_entry_type() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        std::fs::write(tmp.path().join("evidence").join("a.txt"), "alpha").unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file_with_type(&conn, "f-a", &ds_id, "a.txt", "File");
        drop(conn);

        assert_eq!(count_analysis_file_tasks(&db_path, &ds_id).unwrap(), 1);
        let page = fetch_analysis_file_page(
            &persistence_sqlite::open_or_create(&db_path).unwrap(),
            &ds_id,
            0,
            10,
        )
        .unwrap();
        assert_eq!(page.len(), 1);

        let stats = run_import_analysis_staging(
            analysis_options(
                &tmp,
                db_path,
                ds_id.clone(),
                ImportAnalysisMode::BudgetedContent,
            ),
            None,
        )
        .unwrap();
        assert_eq!(stats.processed_count, 1);
        assert!(stats.timeline_count > 0);
    }

    #[test]
    fn analysis_indexing_skips_large_or_unknown_extension_files() {
        let small_text = FileEntry {
            id: FileEntryId("small".to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds".to_string()),
            path: "small.txt".to_string(),
            name: "small.txt".to_string(),
            entry_type: EntryType::File,
            size: Some(512),
            ext: Some("txt".to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        };
        let large_text = FileEntry {
            size: Some(infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES + 1),
            ..small_text.clone()
        };
        let unknown = FileEntry {
            path: "blob.bin".to_string(),
            name: "blob.bin".to_string(),
            ext: Some("bin".to_string()),
            ..small_text.clone()
        };

        assert!(should_index_file(&small_text));
        assert!(!should_index_file(&large_text));
        assert!(!should_index_file(&unknown));
    }

    #[test]
    fn analysis_text_indexing_raw_exfat_uses_bytes_only_reader() {
        let _hook_guard = TEST_HOOK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = TempDir::new().unwrap();
        let raw_path = tmp.path().join("text-exfat.raw");
        write_exfat_text_raw_fixture(&raw_path).unwrap();

        let db_path = tmp.path().join("app.db");
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-raw-exfat-index', 'Raw exFAT Index Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let ds_id = DataSourceId("ds-raw-exfat-index".to_string());
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-raw-exfat-index".to_string()),
                &DataSource {
                    id: ds_id.clone(),
                    name: "raw exfat evidence".to_string(),
                    kind: DataSourceKind::Raw,
                    source_path: raw_path,
                    imported_at: chrono::Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .unwrap();
        conn.execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('file-raw-exfat-index', NULL, ?1, 'LARGE.TXT', 'LARGE.TXT', 'file', 1536, 'txt', 0, 0, 0)",
            params![ds_id.0],
        )
        .unwrap();
        drop(conn);

        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::BudgetedContent,
        );
        options.enable_timeline_projection = false;
        options.enable_content_extraction = false;
        options.enable_text_indexing = true;

        test_hooks::reset();
        test_hooks::track_file_id("file-raw-exfat-index");
        let stats = run_import_analysis_staging(options, None).unwrap();

        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.artifact_count, 0);
        assert_eq!(stats.indexed_count, 1);
        assert_eq!(stats.warning_count, 0);
        assert_eq!(test_hooks::text_index_bytes_reads(), 1);

        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        let (text, truncated): (String, i32) = worker_conn
            .query_row(
                "SELECT text, truncated FROM index_docs WHERE file_id = 'file-raw-exfat-index'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(text.starts_with("AAAA"));
        assert_eq!(truncated, 0);
    }

    #[test]
    fn analysis_worker_reuses_preview_descriptor_cache_across_content_reads() {
        let _hook_guard = TEST_HOOK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("evidence");
        let prefetch_dir = evidence_dir.join("Windows").join("Prefetch");
        std::fs::create_dir_all(&prefetch_dir).unwrap();
        let file_name = "APP.EXE-12345678.pf";
        std::fs::write(prefetch_dir.join(file_name), b"fake prefetch text").unwrap();

        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        conn.execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('file-cache-pf', NULL, ?1, 'Windows/Prefetch/APP.EXE-12345678.pf',
                     'APP.EXE-12345678.pf', 'file', 17, 'txt', 0, 0, 0)",
            params![ds_id.0],
        )
        .unwrap();
        drop(conn);

        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::BudgetedContent,
        );
        options.enable_timeline_projection = false;
        options.enable_content_extraction = true;
        options.enable_text_indexing = true;

        crate::file_service::reset_preview_descriptor_for_case_call_count();
        test_hooks::reset();
        test_hooks::track_file_id("file-cache-pf");
        let stats = run_import_analysis_staging(options, None).unwrap();

        assert_eq!(stats.processed_count, 1);
        assert_eq!(test_hooks::artifact_bytes_reads(), 1);
        assert_eq!(test_hooks::text_index_bytes_reads(), 1);
        assert_eq!(
            crate::file_service::preview_descriptor_for_case_call_count(),
            1,
            "the worker should reuse one preview descriptor across artifact and text reads"
        );
    }

    #[test]
    fn analysis_artifact_extraction_raw_exfat_uses_bytes_only_reader() {
        let _hook_guard = TEST_HOOK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = TempDir::new().unwrap();
        let raw_path = tmp.path().join("artifact-exfat.raw");
        let image_file_name = "$IABCDEF";
        write_exfat_single_file_raw_fixture(
            &raw_path,
            image_file_name,
            &recycle_bin_i_file_bytes("C:\\Users\\alice\\Desktop\\deleted.txt"),
        )
        .unwrap();

        let db_path = tmp.path().join("app.db");
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-raw-exfat-artifact', 'Raw exFAT Artifact Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let ds_id = DataSourceId("ds-raw-exfat-artifact".to_string());
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-raw-exfat-artifact".to_string()),
                &DataSource {
                    id: ds_id.clone(),
                    name: "raw exfat evidence".to_string(),
                    kind: DataSourceKind::Raw,
                    source_path: raw_path,
                    imported_at: chrono::Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .unwrap();
        conn.execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES (?1, NULL, ?2, '$Recycle.Bin/$IABCDEF', '$IABCDEF', 'file', ?3, NULL, 0, 0, 0)",
            params![
                image_file_name,
                ds_id.0,
                (infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES + 4096) as i64
            ],
        )
        .unwrap();
        drop(conn);

        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::FullContent,
        );
        options.enable_timeline_projection = false;
        options.enable_content_extraction = true;
        options.enable_text_indexing = false;

        test_hooks::reset();
        test_hooks::track_file_id(image_file_name);
        let stats = run_import_analysis_staging(options, None).unwrap();

        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.artifact_count, 1);
        assert_eq!(stats.indexed_count, 0);
        assert_eq!(stats.warning_count, 0);
        assert_eq!(test_hooks::artifact_bytes_reads(), 1);
        assert_eq!(test_hooks::text_index_bytes_reads(), 0);

        let main_conn = persistence_sqlite::open_or_create(&tmp.path().join("app.db")).unwrap();
        let (artifact_type, source_object_id, summary): (String, String, String) = main_conn
            .query_row(
                "SELECT artifact_type, source_object_id, summary
                 FROM artifacts
                 WHERE source_object_id = '$IABCDEF'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(artifact_type, "RecycleBin");
        assert_eq!(source_object_id, image_file_name);
        assert!(summary.contains("deleted.txt"));
    }

    #[test]
    fn analysis_artifact_extraction_skips_large_candidates() {
        let registry = artifact_service::create_registry();
        let small_prefetch = FileEntry {
            id: FileEntryId("small-pf".to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds".to_string()),
            path: "Windows/Prefetch/APP.EXE-12345678.pf".to_string(),
            name: "APP.EXE-12345678.pf".to_string(),
            entry_type: EntryType::File,
            size: Some(128 * 1024),
            ext: Some("pf".to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        };
        let large_prefetch = FileEntry {
            size: Some(infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES + 1),
            ..small_prefetch.clone()
        };
        let non_candidate = FileEntry {
            path: "notes.txt".to_string(),
            name: "notes.txt".to_string(),
            ext: Some("txt".to_string()),
            ..small_prefetch.clone()
        };

        assert!(should_extract_artifact(&registry, &small_prefetch));
        assert!(!should_extract_artifact(&registry, &large_prefetch));
        assert!(!should_extract_artifact(&registry, &non_candidate));
    }

    #[test]
    fn disabled_import_content_reads_keep_analysis_bounded() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "missing-text", &ds_id, "missing.txt");
        insert_file(
            &conn,
            "missing-pf",
            &ds_id,
            "Windows/Prefetch/MISSING.EXE-12345678.pf",
        );
        drop(conn);

        let stats = run_import_analysis_staging(
            analysis_options(
                &tmp,
                db_path,
                ds_id.clone(),
                ImportAnalysisMode::MetadataOnly,
            ),
            None,
        )
        .unwrap();

        assert_eq!(stats.processed_count, 2);
        assert!(stats.timeline_count > 0);
        assert_eq!(stats.artifact_count, 0);
        assert_eq!(stats.indexed_count, 0);
        assert_eq!(stats.warning_count, 0);
        assert_eq!(stats.skipped_count, 0);
    }

    #[test]
    fn analysis_warning_partial_semantics_are_preserved_after_startup_guard() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "missing-text", &ds_id, "missing.txt");
        drop(conn);

        let stats = run_import_analysis_staging(
            analysis_options(
                &tmp,
                db_path,
                ds_id.clone(),
                ImportAnalysisMode::BudgetedContent,
            ),
            None,
        )
        .unwrap();

        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.warning_count, 1);
        assert_eq!(stats.skipped_count, 1);
        assert_eq!(stats.failed_count, 0);
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "status")
                .unwrap()
                .as_deref(),
            Some("done")
        );
    }

    #[test]
    fn cancelled_analysis_keeps_cancel_error_and_unmerged_worker_status() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        drop(conn);

        let cancel = Arc::new(AtomicBool::new(true));
        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        options.cancel_token = cancel;

        let result = run_import_analysis_staging(options, None);

        assert!(matches!(result, Err(ref error) if error.to_string().contains("cancelled")));
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "status")
                .unwrap()
                .as_deref(),
            Some("cancelled")
        );
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("false")
        );
    }

    #[test]
    fn producer_never_buffers_more_than_queue_bound() {
        assert_eq!(analysis_task_queue_bound(1), 256);
        assert_eq!(analysis_task_queue_bound(4), 1024);
    }

    #[test]
    fn content_budget_blocks_large_file_and_disabled_mode() {
        let shared = SharedAnalysisState::new();
        let file = FileEntry {
            id: FileEntryId("large".to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds".to_string()),
            path: "large.txt".to_string(),
            name: "large.txt".to_string(),
            entry_type: EntryType::File,
            size: Some(infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES + 1),
            ext: Some("txt".to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        };

        assert!(!reserve_content_budget(
            &ContentBudget::disabled(),
            &file,
            &shared
        ));
        assert!(!reserve_content_budget(
            &ContentBudget::conservative(),
            &file,
            &shared
        ));
    }

    #[test]
    fn analysis_memory_guard_cancels_over_limit() {
        let tmp = TempDir::new().unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut options = analysis_options(&tmp, db_path, ds_id, ImportAnalysisMode::MetadataOnly);
        options.cancel_token = cancel.clone();
        options.memory_soft_limit_mb = 1;
        options.memory_hard_limit_mb = 2;
        set_test_rss_override_mb(Some(3));

        let result = run_import_analysis_staging(options, None);
        set_test_rss_override_mb(None);

        assert!(result.is_err());
        assert!(cancel.load(Ordering::Relaxed));
    }
}
