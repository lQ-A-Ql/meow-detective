use persistence_sqlite::{open_in_memory, repositories::staging_repo::StagingRepo, runner};
use rusqlite::{Connection, ErrorCode};
use tempfile::TempDir;

const APPLICATION_ENCRYPTION_MIGRATION: &str =
    include_str!("../src/migrations/scripts/0042_file_entry_encrypted.sql");
const SOURCE_ENCRYPTION_MIGRATION: &str =
    include_str!("../src/migrations/scripts/source_025_file_entry_encrypted.sql");

#[test]
fn application_and_source_schemas_share_the_encrypted_column_contract() {
    let application = open_in_memory().expect("open application database");
    runner::run_all(&application).expect("run application migrations");
    seed_application_source(&application);
    assert_encrypted_column_contract(&application, "app-file", "app-source");

    let source = open_in_memory().expect("open source database");
    runner::run_source_all(&source).expect("run source migrations");
    seed_source_local_registration(&source);
    assert_encrypted_column_contract(&source, "source-file", "source-1");
}

#[test]
fn fresh_and_upgraded_staging_schemas_share_the_encrypted_column_contract() {
    let temporary = TempDir::new().expect("create staging root");
    let fresh = StagingRepo::open_partition_staging_conn(temporary.path(), "fresh", 0)
        .expect("open fresh staging database");
    assert_encrypted_column_contract(&fresh, "fresh-file", "fresh");

    let upgraded = Connection::open_in_memory().expect("open legacy staging database");
    upgraded
        .execute_batch(
            "CREATE TABLE file_entries (
                id TEXT PRIMARY KEY NOT NULL,
                parent_id TEXT,
                data_source_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                size INTEGER,
                ext TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                modified_at TEXT,
                accessed_at TEXT,
                changed_at TEXT,
                hash_sha256 TEXT
            );
            INSERT INTO file_entries
                (id, data_source_id, path, name, entry_type)
            VALUES ('legacy-stage', 'upgraded', 'old.bin', 'old.bin', 'file');",
        )
        .expect("create legacy staging schema");
    StagingRepo::ensure_enum_staging_columns(&upgraded).expect("upgrade staging schema");
    let historical_status: Option<i64> = upgraded
        .query_row(
            "SELECT encrypted FROM file_entries WHERE id = 'legacy-stage'",
            [],
            |row| row.get(0),
        )
        .expect("read upgraded staging status");
    assert_eq!(historical_status, None);
    assert_encrypted_column_contract(&upgraded, "upgraded-file", "upgraded");
}

#[test]
fn historical_rows_remain_unknown_after_application_and_source_migrations() {
    for migration in [
        APPLICATION_ENCRYPTION_MIGRATION,
        SOURCE_ENCRYPTION_MIGRATION,
    ] {
        let connection = Connection::open_in_memory().expect("open legacy database");
        connection
            .execute_batch(
                "CREATE TABLE file_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    data_source_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    name TEXT NOT NULL,
                    entry_type TEXT NOT NULL
                );
                INSERT INTO file_entries
                    (id, data_source_id, path, name, entry_type)
                VALUES ('historical', 'source-1', 'old.bin', 'old.bin', 'file');",
            )
            .expect("seed historical row");

        connection
            .execute_batch(migration)
            .expect("apply encryption migration");

        let status: Option<i64> = connection
            .query_row(
                "SELECT encrypted FROM file_entries WHERE id = 'historical'",
                [],
                |row| row.get(0),
            )
            .expect("read migrated state");
        assert_eq!(status, None);
    }
}

fn assert_encrypted_column_contract(conn: &Connection, file_id: &str, data_source_id: &str) {
    let column: (String, i64, Option<String>) = conn
        .query_row(
            "SELECT type, \"notnull\", dflt_value
             FROM pragma_table_info('file_entries')
             WHERE name = 'encrypted'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("encrypted column");
    assert_eq!(column, ("INTEGER".to_string(), 0, None));

    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type)
         VALUES (?1, ?2, 'plain.txt', 'plain.txt', 'file')",
        (file_id, data_source_id),
    )
    .expect("insert ordinary file with default encryption state");
    let encrypted: Option<i64> = conn
        .query_row(
            "SELECT encrypted FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read default encryption state");
    assert_eq!(
        encrypted, None,
        "omitted classification must remain unknown"
    );

    conn.execute(
        "UPDATE file_entries SET encrypted = 0 WHERE id = ?1",
        [file_id],
    )
    .expect("persist explicit clear state");
    let clear: i64 = conn
        .query_row(
            "SELECT encrypted FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read clear state");
    assert_eq!(clear, 0);

    conn.execute(
        "UPDATE file_entries SET encrypted = 1 WHERE id = ?1",
        [file_id],
    )
    .expect("persist explicit encrypted state");

    let error = conn
        .execute(
            "UPDATE file_entries SET encrypted = 2 WHERE id = ?1",
            [file_id],
        )
        .expect_err("reject a non-boolean encryption state");
    assert!(matches!(
        error.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    ));
}

fn seed_application_source(conn: &Connection) {
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-1', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert application case");
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('app-source', 'case-1', 'source', 'e01', '', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert application source");
}

fn seed_source_local_registration(conn: &Connection) {
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('source-1', 'case-1', 'source', 'ceph_rbd', '', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert source-local registration");
}
