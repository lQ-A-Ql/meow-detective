use persistence_sqlite::{open_in_memory, runner};

#[test]
fn source_schema_creates_android_analysis_tables() {
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

    assert_eq!(table_names, ["android_packages", "android_system_facts"]);
    assert_eq!(
        runner::latest_source_version(),
        "source_037_forensic_fingerprints"
    );
}
