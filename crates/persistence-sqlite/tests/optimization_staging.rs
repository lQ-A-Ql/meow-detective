use persistence_sqlite::repositories::{
    ntfs_staging_repo::{NtfsStagingRepo, NTFS_STAGING_LAYOUT_VERSION},
    source_bulk_load_repo::{BulkLoadState, SourceBulkLoadRepo},
};
use rusqlite::Connection;
#[test]
fn numeric_layout_is_versioned_and_clear_preserves_layout() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE source_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL)")
        .unwrap();
    NtfsStagingRepo::ensure_schema(&c).unwrap();
    assert_eq!(
        NtfsStagingRepo::meta(&c, "layout_version")
            .unwrap()
            .as_deref(),
        Some(NTFS_STAGING_LAYOUT_VERSION)
    );
    NtfsStagingRepo::set_meta(&c, "attempt", "a").unwrap();
    NtfsStagingRepo::clear(&c).unwrap();
    assert_eq!(NtfsStagingRepo::meta(&c, "attempt").unwrap(), None)
}
#[test]
fn bulk_state_transitions_are_durable() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE source_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL)")
        .unwrap();
    SourceBulkLoadRepo::set_state(&c, BulkLoadState::Active, "a").unwrap();
    assert_eq!(
        SourceBulkLoadRepo::state(&c).unwrap(),
        BulkLoadState::Active
    );
    SourceBulkLoadRepo::set_state(&c, BulkLoadState::Rebuilding, "a").unwrap();
    assert_eq!(
        SourceBulkLoadRepo::state(&c).unwrap(),
        BulkLoadState::Rebuilding
    );
    SourceBulkLoadRepo::set_state(&c, BulkLoadState::Ready, "a").unwrap();
    SourceBulkLoadRepo::clear_transient(&c).unwrap();
    assert_eq!(
        SourceBulkLoadRepo::state(&c).unwrap(),
        BulkLoadState::Inactive
    )
}

fn indexes(conn: &Connection) -> Vec<(String, String)> {
    conn.prepare("SELECT name,sql FROM sqlite_master WHERE type='index' AND tbl_name='file_entries' AND sql IS NOT NULL ORDER BY name").unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap()
}

#[test]
fn bulk_load_restores_exact_indexes_after_reopening_an_interrupted_import() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("source.db");
    let source = persistence_sqlite::open_or_create_source(&path).unwrap();
    let original = indexes(&source);
    assert!(SourceBulkLoadRepo::begin(&source, "interrupted").unwrap());
    assert!(indexes(&source).len() < original.len());
    drop(source);
    let source = persistence_sqlite::open_existing_source(&path).unwrap();
    assert_eq!(
        SourceBulkLoadRepo::state(&source).unwrap(),
        BulkLoadState::Active
    );
    SourceBulkLoadRepo::recover(&source).unwrap();
    assert_eq!(indexes(&source), original);
    assert_eq!(
        SourceBulkLoadRepo::state(&source).unwrap(),
        BulkLoadState::Inactive
    );
    SourceBulkLoadRepo::recover(&source).unwrap();
}

#[test]
fn bulk_rebuild_failure_never_publishes_ready() {
    let source = persistence_sqlite::open_in_memory().unwrap();
    source.execute_batch("CREATE TABLE source_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE file_entries(id TEXT PRIMARY KEY)").unwrap();
    SourceBulkLoadRepo::begin(&source, "failed").unwrap();
    source
        .execute(
            "UPDATE source_meta SET value=?1 WHERE key='bulk_load.indexes'",
            [r#"[["missing","CREATE INDEX missing ON absent(id)"]]"#],
        )
        .unwrap();
    assert!(SourceBulkLoadRepo::finish(&source).is_err());
    assert_eq!(
        SourceBulkLoadRepo::state(&source).unwrap(),
        BulkLoadState::Rebuilding
    );
}

#[test]
fn numeric_staging_projects_ids_and_resets_atomically_for_generic_fallback() {
    let temporary = tempfile::tempdir().unwrap();
    let source =
        persistence_sqlite::repositories::staging_repo::StagingRepo::open_partition_staging_conn(
            temporary.path(),
            "source",
            2,
        )
        .unwrap();
    NtfsStagingRepo::begin(&source, "source", 2).unwrap();
    source.execute_batch("INSERT INTO ntfs_entries(record_num,sequence_num,name,entry_type,deleted,hidden,system,read_only,archive) VALUES(5,7,'root',1,0,0,0,0,0)").unwrap();
    let id: String = source
        .query_row("SELECT id FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(id, "mft:2:5");
    persistence_sqlite::repositories::staging_repo::StagingRepo::reset_enum_for_fallback(&source)
        .unwrap();
    assert!(!NtfsStagingRepo::is_numeric(&source).unwrap());
    source.execute_batch("INSERT INTO file_entries(id,data_source_id,path,name,entry_type) VALUES('generic','source','','root','directory')").unwrap();
    assert_eq!(
        source
            .query_row("SELECT COUNT(*) FROM ntfs_entries", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
