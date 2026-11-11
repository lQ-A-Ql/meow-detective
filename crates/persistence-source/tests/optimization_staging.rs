use persistence_sqlite::repositories::ntfs_staging_repo::{
    NtfsStagingRepo, NTFS_STAGING_LAYOUT_VERSION,
};
use rusqlite::Connection;
#[test]
fn numeric_layout_is_versioned_and_clear_preserves_layout() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!(
        "../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
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
fn numeric_staging_projects_ids_and_resets_atomically_for_generic_fallback() {
    let temporary = tempfile::tempdir().unwrap();
    let source =
        persistence_source::repositories::staging_repo::StagingRepo::open_partition_staging_conn(
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
    persistence_source::repositories::staging_repo::StagingRepo::reset_enum_for_fallback(&source)
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
