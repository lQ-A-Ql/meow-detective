use persistence_sqlite::repositories::{
    ntfs_staging_repo::{NtfsStagingRepo as Repo, NTFS_STAGING_LAYOUT_VERSION},
    staging_repo::StagingRepo,
};
use rusqlite::Connection;

fn legacy(c: &Connection) {
    c.execute_batch(include_str!("../src/migrations/scripts/staging_001.sql"))
        .unwrap();
    c.execute_batch("CREATE TABLE ntfs_staging_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO ntfs_staging_meta VALUES('layout_version','ntfs-v2'),('source_id','old'),('partition','2'); DROP TABLE file_entries;").unwrap();
    c.execute_batch(include_str!("fixtures/ntfs_v2_schema.sql"))
        .unwrap();
    c.execute_batch(include_str!("fixtures/ntfs_v2_projection.sql"))
        .unwrap();
    c.execute_batch("INSERT INTO ntfs_entries(record_num,sequence_num,name,entry_type,deleted,hidden,system,read_only,archive) VALUES(5,7,'\\',1,0,0,0,0,0); INSERT INTO staging_meta VALUES('status','done'),('enum_strategy','mft'),('file_count','77');").unwrap();
}
#[test]
fn known_legacy_catalog_is_reenumerated_without_migrating_published_source() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("staging/source");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("enum_partition_2.db");
    let c = Connection::open(&path).unwrap();
    legacy(&c);
    drop(c);
    let c = StagingRepo::open_partition_staging_conn(temp.path(), "source", 2).unwrap();
    assert!(!Repo::is_numeric(&c).unwrap());
    assert_eq!(
        Repo::meta(&c, "layout_version").unwrap().as_deref(),
        Some(NTFS_STAGING_LAYOUT_VERSION)
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM file_entries", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(StagingRepo::get_staging_meta(&c, "status").unwrap(), None);
    Repo::begin(&c, "new", 2).unwrap();
    assert!(Repo::validate_paths(&c).is_err());
    let source = persistence_sqlite::open_or_create_source(&temp.path().join("source.db")).unwrap();
    assert!(Repo::ensure_schema(&source).is_err());
    assert_eq!(
        source
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='ntfs_staging_meta'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
}
#[test]
fn failed_legacy_reset_keeps_the_view_and_done_marker() {
    let c = Connection::open_in_memory().unwrap();
    legacy(&c);
    c.execute_batch("CREATE TRIGGER reject_reset BEFORE DELETE ON staging_meta BEGIN SELECT RAISE(ABORT,'failure'); END").unwrap();
    assert!(Repo::rebuild_previous_layout(&c).is_err());
    assert!(c.is_autocommit());
    assert!(Repo::is_numeric(&c).unwrap());
    assert_eq!(
        Repo::meta(&c, "layout_version").unwrap().as_deref(),
        Some("ntfs-v2")
    );
    assert_eq!(
        StagingRepo::get_staging_meta(&c, "status")
            .unwrap()
            .as_deref(),
        Some("done")
    );
    assert_eq!(
        c.query_row("SELECT id FROM file_entries", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "mft:2:5"
    );
}
#[test]
fn computed_null_parent_and_representative_do_not_mutate_raw_facts() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!("../src/migrations/scripts/staging_001.sql"))
        .unwrap();
    Repo::begin(&c, "source", 3).unwrap();
    c.execute_batch("INSERT INTO ntfs_entries(record_num,parent_record_num,sequence_num,name,entry_type,size,ext,deleted,hidden,system,read_only,archive) VALUES(5,5,7,'raw-root',1,NULL,NULL,0,0,0,0,0),(42,999,8,'SHORT.TXT',0,12,'txt',0,0,0,0,0)").unwrap();
    assert!(Repo::validate_paths(&c).is_err());
    Repo::write_paths(&c, |s| -> rusqlite::Result<()> {
        s.execute(rusqlite::params![5, "", "\\", Option::<u64>::None, 0])?;
        s.execute(rusqlite::params![42, "Long.bin", "Long.bin", 5, 1])?;
        Ok(())
    })
    .unwrap();
    Repo::validate_paths(&c).unwrap();
    assert_eq!(Repo::tree_shape(&c).unwrap(), (2, 1, 0, 2));
    assert_eq!(
        c.query_row(
            "SELECT parent_id FROM file_entries WHERE id='mft:3:5'",
            [],
            |r| r.get::<_, Option<String>>(0)
        )
        .unwrap(),
        None
    );
    assert_eq!(
        c.query_row(
            "SELECT name,ext,parent_id FROM file_entries WHERE id='mft:3:42'",
            [],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?
            ))
        )
        .unwrap(),
        ("Long.bin".into(), "txt".into(), "mft:3:5".into())
    );
    assert_eq!(
        c.query_row(
            "SELECT name,parent_record_num,sequence_num FROM ntfs_entries WHERE record_num=42",
            [],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u64>(1)?,
                r.get::<_, u64>(2)?
            ))
        )
        .unwrap(),
        ("SHORT.TXT".into(), 999, 8)
    );
}
