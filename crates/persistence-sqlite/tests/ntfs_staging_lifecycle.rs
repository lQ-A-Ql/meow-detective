use persistence_sqlite::repositories::{
    ntfs_staging_repo::NtfsStagingRepo as Repo, staging_repo::StagingRepo,
};
use rusqlite::Connection;

fn staging() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!("../src/migrations/scripts/staging_001.sql"))
        .unwrap();
    Repo::ensure_schema(&c).unwrap();
    c
}
fn root(c: &Connection) {
    c.execute_batch("INSERT INTO ntfs_entries(record_num,parent_record_num,sequence_num,name,entry_type,deleted,hidden,system,read_only,archive) VALUES(5,NULL,7,'root',1,0,0,0,0,0)").unwrap();
    Repo::write_paths(c, |s| {
        s.execute(rusqlite::params![5, "", "root", Option::<u64>::None, 0])
    })
    .unwrap();
}
fn indexes(c: &Connection) -> Vec<String> {
    c.prepare("SELECT name FROM sqlite_master WHERE type='index' AND tbl_name IN ('ntfs_entries','ntfs_paths') ORDER BY name").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap()
}
#[test]
fn parent_index_belongs_to_validation_not_next_ingest() {
    let c = staging();
    for (source, partition) in [("first", 1), ("second", 9)] {
        Repo::begin(&c, source, partition).unwrap();
        assert!(!indexes(&c).iter().any(|i| i == "ntfs_parent"));
        root(&c);
        assert_eq!(Repo::tree_shape(&c).unwrap(), (1, 1, 0, 1));
        assert!(indexes(&c).iter().any(|i| i == "ntfs_parent"));
        let id: String = c
            .query_row("SELECT id FROM file_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, format!("mft:{partition}:5"));
        Repo::reset_layout(&c).unwrap();
        assert!(!Repo::is_numeric(&c).unwrap());
        assert!(!indexes(&c).iter().any(|i| i == "ntfs_parent"));
        assert_eq!(Repo::meta(&c, "source_id").unwrap(), None);
    }
}
#[test]
fn clearing_failure_restores_rows_metadata_and_validation_index() {
    let c = staging();
    Repo::begin(&c, "source", 1).unwrap();
    root(&c);
    Repo::tree_shape(&c).unwrap();
    c.execute_batch("CREATE TRIGGER reject_clear BEFORE DELETE ON ntfs_staging_meta BEGIN SELECT RAISE(ABORT,'fail'); END").unwrap();
    assert!(Repo::clear(&c).is_err());
    assert!(c.is_autocommit());
    assert!(indexes(&c).iter().any(|i| i == "ntfs_parent"));
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM ntfs_entries", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        Repo::meta(&c, "source_id").unwrap().as_deref(),
        Some("source")
    );
}
#[test]
fn reset_inside_cancelled_transaction_rolls_back_ddl_and_can_retry() {
    let c = staging();
    Repo::begin(&c, "old", 1).unwrap();
    root(&c);
    Repo::tree_shape(&c).unwrap();
    c.execute_batch("BEGIN").unwrap();
    Repo::reset_layout(&c).unwrap();
    Repo::begin(&c, "cancelled", 2).unwrap();
    root(&c);
    c.execute_batch("ROLLBACK").unwrap();
    assert_eq!(Repo::meta(&c, "source_id").unwrap().as_deref(), Some("old"));
    assert!(indexes(&c).iter().any(|i| i == "ntfs_parent"));
    StagingRepo::reset_enum_for_fallback(&c).unwrap();
    Repo::begin(&c, "retry", 3).unwrap();
    root(&c);
    assert_eq!(
        Repo::meta(&c, "source_id").unwrap().as_deref(),
        Some("retry")
    );
    assert!(!indexes(&c).iter().any(|i| i == "ntfs_parent"));
}
#[test]
fn unknown_layout_cannot_be_cleared_or_silently_adopted() {
    let c = staging();
    Repo::begin(&c, "source", 1).unwrap();
    root(&c);
    Repo::set_meta(&c, "layout_version", "future-layout").unwrap();
    assert!(Repo::clear(&c).is_err());
    assert!(Repo::reset_layout(&c).is_err());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM file_entries", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        Repo::meta(&c, "layout_version").unwrap().as_deref(),
        Some("future-layout")
    );
}

#[test]
fn failed_outermost_savepoint_commit_restores_generic_layout() {
    let c = staging();
    c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE guard_parent(id INTEGER PRIMARY KEY); CREATE TABLE guard_child(id INTEGER REFERENCES guard_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER reject_identity AFTER INSERT ON ntfs_staging_meta WHEN NEW.key='source_id' BEGIN INSERT INTO guard_child VALUES(7); END;").unwrap();
    assert!(Repo::begin(&c, "failed", 1).is_err());
    assert!(c.is_autocommit());
    assert!(!Repo::is_numeric(&c).unwrap());
    assert_eq!(Repo::meta(&c, "source_id").unwrap(), None);
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM guard_child", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
}
