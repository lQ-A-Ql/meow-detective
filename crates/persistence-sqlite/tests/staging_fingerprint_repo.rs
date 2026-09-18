use persistence_sqlite::{repositories::staging_fingerprint_repo, runner};
use rusqlite::params;

#[test]
fn analysis_staging_artifacts_receive_fingerprints_in_source_database() {
    let conn = persistence_sqlite::open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    conn.execute_batch(
        "ATTACH DATABASE ':memory:' AS analysis_stage;
         CREATE TABLE analysis_stage.artifact_rows (
             id TEXT PRIMARY KEY NOT NULL,
             file_id TEXT,
             extractor_id TEXT,
             extractor_version TEXT,
             source_attribution TEXT
         );
         INSERT INTO analysis_stage.artifact_rows
             (id, file_id, extractor_id, extractor_version, source_attribution)
         VALUES ('artifact-1', 'file-1', 'registry', '1', 'SYSTEM\\ControlSet001');",
    )
    .expect("create analysis staging rows");

    staging_fingerprint_repo::persist_analysis_fingerprints(&conn, "case-1", "source-1")
        .expect("persist artifact fingerprint");

    let row: (String, String, String) = conn
        .query_row(
            "SELECT object_id, source_id, source_locator
             FROM forensic_fingerprints
             WHERE object_id = ?1",
            params!["artifact-1"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read artifact fingerprint");
    assert_eq!(
        row,
        (
            "artifact-1".into(),
            "source-1".into(),
            "SYSTEM\\ControlSet001".into()
        )
    );
}
