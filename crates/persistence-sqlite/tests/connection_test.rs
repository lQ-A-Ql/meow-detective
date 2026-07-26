use persistence_sqlite::{open_in_memory, runner};
use tempfile::TempDir;

#[test]
fn create_new_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
    assert!(db_path.exists());

    let count = runner::run_all(&conn).unwrap();
    assert_eq!(count as usize, runner::migration_count());
}

#[test]
fn open_existing_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    {
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        runner::run_all(&conn).unwrap();
    }
    {
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        let count = runner::run_all(&conn).unwrap();
        assert_eq!(count, 0, "Re-opening should not re-apply migrations");
    }
}

#[test]
fn run_all_migrations() {
    let conn = open_in_memory().unwrap();
    let count = runner::run_all(&conn).unwrap();
    assert_eq!(count as usize, runner::migration_count());

    let tables = [
        "cases",
        "data_sources",
        "file_entries",
        "artifacts",
        "timeline_events",
        "jobs",
        "reports",
        "tags",
        "data_source_partitions",
    ];
    for table in &tables {
        let has_table: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_table, "Table '{}' should exist after migration", table);
    }
}

#[test]
fn idempotent_rerun() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    let count = runner::run_all(&conn).unwrap();
    assert_eq!(count, 0, "Second run should not apply any migrations");
}

#[test]
fn latest_marker_does_not_hide_missing_earlier_migrations() {
    let conn = open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (name) VALUES (?1)",
        [runner::latest_version()],
    )
    .unwrap();

    let applied = runner::run_all(&conn).unwrap();

    assert_eq!(applied as usize, runner::migration_count() - 1);
    let cases_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'cases'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(cases_exists);
}

#[test]
fn version_query() {
    let conn = open_in_memory().unwrap();
    let version = runner::current_version(&conn).unwrap();
    assert!(version.is_none());

    runner::run_all(&conn).unwrap();
    let version = runner::current_version(&conn).unwrap();
    assert_eq!(version, Some(runner::latest_version().to_string()));
}

#[test]
fn tables_exist_after_migration() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='file_entries'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(count > 0, "file_entries table should exist");
}

#[test]
fn failed_migration_is_not_marked_applied() {
    let conn = open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT INTO schema_migrations (name) VALUES ('0001_cases');
        CREATE TABLE cases (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            number TEXT,
            examiner TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE data_sources (
            id TEXT PRIMARY KEY NOT NULL
        );",
    )
    .unwrap();

    let err = runner::run_all(&conn).unwrap_err();
    assert!(format!("{err}").contains("0002_data_sources"));

    let applied: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE name = '0002_data_sources'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(applied, 0);
}

#[test]
fn upgrades_legacy_schema_to_latest_with_partition_job_columns() {
    let conn = open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT INTO schema_migrations (name) VALUES
            ('0001_cases'),
            ('0002_data_sources'),
            ('0003_file_entries'),
            ('0004_artifacts'),
            ('0005_timeline_events'),
            ('0006_jobs'),
            ('0007_reports'),
            ('0008_tags'),
            ('0009_data_source_partitions');

        CREATE TABLE cases (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            number TEXT,
            examiner TEXT,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE data_sources (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id),
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            source_path TEXT NOT NULL,
            imported_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE file_entries (
            id TEXT PRIMARY KEY NOT NULL,
            parent_id TEXT REFERENCES file_entries(id),
            data_source_id TEXT NOT NULL,
            path TEXT NOT NULL,
            name TEXT NOT NULL,
            entry_type TEXT NOT NULL,
            size INTEGER,
            ext TEXT,
            deleted INTEGER NOT NULL DEFAULT 0,
            hidden INTEGER NOT NULL DEFAULT 0,
            system INTEGER NOT NULL DEFAULT 0,
            created_at TEXT,
            modified_at TEXT,
            accessed_at TEXT,
            changed_at TEXT,
            hash_sha256 TEXT
        );
        CREATE TABLE artifacts (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL DEFAULT '',
            data_source_id TEXT NOT NULL DEFAULT '',
            artifact_type TEXT NOT NULL,
            source_object_id TEXT,
            title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            attrs TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE timeline_events (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL DEFAULT '',
            source_object_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            ts TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            attrs TEXT NOT NULL DEFAULT '{}'
        );
        CREATE TABLE jobs (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id),
            kind TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            progress INTEGER NOT NULL DEFAULT 0,
            detail TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            started_at TEXT,
            finished_at TEXT
        );
        CREATE TABLE reports (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id),
            template_id TEXT NOT NULL,
            file_name TEXT NOT NULL,
            created_by TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'running',
            progress INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE tags (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id),
            name TEXT NOT NULL,
            color TEXT
        );
        CREATE TABLE tag_bindings (
            tag_id TEXT NOT NULL REFERENCES tags(id),
            object_id TEXT NOT NULL,
            PRIMARY KEY (tag_id, object_id)
        );
        CREATE TABLE data_source_partitions (
            id TEXT PRIMARY KEY,
            data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
            partition_index INTEGER NOT NULL,
            name TEXT NOT NULL,
            kind_label TEXT NOT NULL,
            status TEXT NOT NULL,
            type_guid TEXT,
            offset INTEGER NOT NULL,
            length INTEGER NOT NULL,
            filesystem TEXT,
            unlock_hint TEXT
        );

        INSERT INTO cases (id, name, created_at, updated_at)
            VALUES ('case-old', 'Old Case', datetime('now'), datetime('now'));
        INSERT INTO data_sources (id, case_id, name, kind, source_path)
            VALUES ('ds-old', 'case-old', 'Old DS', 'logical_directory', 'C:/evidence');
        INSERT INTO jobs (id, case_id, kind, status, progress, detail)
            VALUES ('job-old', 'case-old', 'Import', 'running', 42, 'legacy');
        INSERT INTO reports (id, case_id, template_id, file_name)
            VALUES ('report-old', 'case-old', 'default', 'report.html');
        INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
            VALUES ('file-old', 'ds-old', 'note.txt', 'note.txt', 'file');",
    )
    .unwrap();

    let applied = runner::run_all(&conn).unwrap();
    assert!(applied > 0);
    assert_eq!(
        runner::current_version(&conn).unwrap(),
        Some(runner::latest_version().to_string())
    );

    for column in [
        "current_partition",
        "completed_partitions",
        "total_partitions",
        "partition_progress",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('jobs') WHERE name = ?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "jobs.{column} should exist after upgrade");
    }

    for column in [
        "lvm_vg_uuid",
        "lvm_vg_name",
        "lvm_lv_uuid",
        "lvm_lv_name",
        "lvm_pv_offsets_json",
        "lvm_pv_sources_json",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('data_source_partitions') WHERE name = ?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            exists,
            "data_source_partitions.{column} should exist after upgrade"
        );
    }

    for (table, column) in [
        ("artifacts", "extractor_id"),
        ("artifacts", "extractor_version"),
        ("artifacts", "confidence"),
        ("artifacts", "source_attribution"),
        ("timeline_events", "parser_id"),
        ("timeline_events", "parser_version"),
        ("timeline_events", "confidence"),
        ("timeline_events", "source_attribution"),
    ] {
        let exists: bool = conn
            .query_row(
                &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?1"),
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "{table}.{column} should exist after upgrade");
    }

    let preserved: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM jobs WHERE id = 'job-old'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(preserved, 1);
}
