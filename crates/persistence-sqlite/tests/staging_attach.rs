use persistence_sqlite::repositories::staging_repo::StagingRepo;

#[test]
fn reopened_staging_can_be_attached_while_its_idle_handle_remains_open() {
    let temporary = tempfile::tempdir().unwrap();
    let staging = StagingRepo::open_partition_staging_conn(temporary.path(), "source", 1).unwrap();
    StagingRepo::set_staging_meta(&staging, "status", "done").unwrap();
    let source = rusqlite::Connection::open_in_memory().unwrap();
    let path = temporary.path().join("staging/source/enum_partition_1.db");
    source
        .execute("ATTACH DATABASE ?1 AS staging", [path.to_str().unwrap()])
        .unwrap();
    source
        .execute_batch("BEGIN IMMEDIATE; SELECT COUNT(*) FROM staging.file_entries; COMMIT;")
        .unwrap();
    source.execute_batch("DETACH DATABASE staging").unwrap();
}
