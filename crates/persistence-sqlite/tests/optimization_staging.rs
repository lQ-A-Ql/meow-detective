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
