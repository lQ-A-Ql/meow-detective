use persistence_sqlite::{open_in_memory, runner};

#[test]
fn source_schema_keeps_android_analysis_in_fjall() {
    let connection = open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");

    let table_names = connection
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name IN ('android_system_facts', 'android_packages')
             ORDER BY name",
        )
        .expect("prepare table query")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query tables")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect tables");

    assert!(table_names.is_empty());
    assert_eq!(
        runner::latest_source_version(),
        "source_042_android_fjall_only"
    );
}

#[test]
fn android_sql_tables_are_removed_only_in_the_owned_migration_fixture() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/android");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let connection = rusqlite::Connection::open(root.path().join("source.db")).unwrap();
    connection
        .execute_batch(include_str!(
            "../src/migrations/scripts/source_036_android_analysis.sql"
        ))
        .unwrap();
    connection.execute("INSERT INTO android_system_facts (id,data_source_id,field_key,field_value,confidence,source_file_id,source_path,parser,source_rank) VALUES ('fact','source','model','value','direct','file','system/build.prop','android',0)", []).unwrap();
    runner::run_source_all(&connection).unwrap();
    let count: u64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('android_system_facts','android_packages')", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        runner::current_version(&connection).unwrap().as_deref(),
        Some("source_042_android_fjall_only")
    );
}
