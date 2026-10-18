use crate::source_db::{
    self, finalize_source_build_db, open_fresh_source_build_db, publish_source_build_db,
};
use domain::DataSourceId;
use rusqlite::Connection;

fn observe(role: &str, conn: &Connection) -> serde_json::Value {
    let mut result = serde_json::json!({"role": role});
    for name in [
        "page_size",
        "cache_size",
        "synchronous",
        "wal_autocheckpoint",
        "temp_store",
        "query_only",
    ] {
        let value: i64 = conn
            .pragma_query_value(None, name, |row| row.get(0))
            .unwrap();
        result[name] = value.into();
    }
    let journal: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    result["journal_mode"] = journal.into();
    result
}

#[test]
fn connection_pragmas_preserve_durable_source_and_disposable_staging_policies() {
    use persistence_sqlite::repositories::staging_repo::StagingRepo;
    let temp = tempfile::tempdir().unwrap();
    let source = source_db::open_source_db(temp.path(), &DataSourceId("ordinary".into())).unwrap();
    let build = open_fresh_source_build_db(
        temp.path(),
        &DataSourceId("build".into()),
        "11111111-1111-4111-8111-111111111111",
    )
    .unwrap();
    let staging = StagingRepo::open_partition_staging_conn(temp.path(), "source", 1).unwrap();
    let read_only = persistence_sqlite::open_existing_source_read_only(&source_db::source_db_path(
        temp.path(),
        &DataSourceId("ordinary".into()),
    ))
    .unwrap();
    let rows = [
        observe("source", &source),
        observe("source-build", &build),
        observe("enum-staging", &staging),
        observe("published-read-only", &read_only),
    ];
    assert_eq!(rows[0]["synchronous"], 1);
    assert_eq!(rows[1]["synchronous"], 1);
    assert_eq!(rows[0]["journal_mode"], "wal");
    assert_eq!(rows[1]["wal_autocheckpoint"], 16 * 1024);
    assert_eq!(rows[2]["page_size"], 8192);
    assert_eq!(rows[2]["synchronous"], 0);
    assert_eq!(rows[2]["cache_size"], -16 * 1024);
    assert_eq!(rows[3]["query_only"], 1);
    assert!(read_only
        .execute(
            "INSERT INTO source_meta(key,value) VALUES ('read-only','no')",
            []
        )
        .is_err());
    println!(
        "[s4-connection-policy] {}",
        serde_json::json!({"connections": rows,
        "scope": "connection PRAGMAs only; WAL/TEMP peaks and physical I/O not measured"})
    );
}

#[test]
fn busy_checkpoint_prevents_build_sealing_and_publication_then_retry_succeeds() {
    let temp = tempfile::tempdir().unwrap();
    let source = DataSourceId("checkpoint-busy".into());
    let attempt = "11111111-1111-4111-8111-111111111111";
    let build_path = source_db::build::source_build_db_path(temp.path(), &source, attempt).unwrap();
    let writer = open_fresh_source_build_db(temp.path(), &source, attempt).unwrap();
    writer.pragma_update(None, "busy_timeout", 0).unwrap();
    writer
        .execute(
            "INSERT INTO source_meta(key,value) VALUES ('payload','first')",
            [],
        )
        .unwrap();
    let reader = persistence_sqlite::open_existing_source_read_only(&build_path).unwrap();
    let snapshot = reader.unchecked_transaction().unwrap();
    let count: u64 = snapshot
        .query_row("SELECT COUNT(*) FROM source_meta", [], |row| row.get(0))
        .unwrap();
    assert!(count > 0);
    writer
        .execute(
            "UPDATE source_meta SET value='second' WHERE key='payload'",
            [],
        )
        .unwrap();
    let error = finalize_source_build_db(&writer).unwrap_err();
    assert!(error.to_string().contains("did not converge"));
    let sealed: u64 = writer
        .query_row(
            "SELECT COUNT(*) FROM source_meta WHERE key='source.build.finalized'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(sealed, 0);
    assert!(publish_source_build_db(temp.path(), &source, attempt).is_err());
    assert!(!source_db::source_db_path(temp.path(), &source).exists());
    drop(snapshot);
    drop(reader);
    finalize_source_build_db(&writer).unwrap();
    drop(writer);
    let published = publish_source_build_db(temp.path(), &source, attempt).unwrap();
    let conn = persistence_sqlite::open_existing_source_read_only(&published).unwrap();
    let value: String = conn
        .query_row(
            "SELECT value FROM source_meta WHERE key='payload'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(value, "second");
}
