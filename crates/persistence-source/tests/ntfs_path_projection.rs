use persistence_source::repositories::staging_repo::StagingRepo;
use persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo as Repo;
use rusqlite::Connection;

#[test]
fn opening_rejects_unsupported_staging_layout_without_rewriting_records() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("staging/source");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("enum_partition_2.db");
    let c = StagingRepo::open_partition_staging_conn(temp.path(), "source", 2).unwrap();
    Repo::begin(&c, "source", 2).unwrap();
    c.execute_batch("INSERT INTO ntfs_entries(record_num,sequence_num,name,entry_type,deleted,hidden,system,read_only,archive) VALUES(5,7,'root',1,0,0,0,0,0); INSERT INTO staging_meta VALUES('status','done');").unwrap();
    Repo::set_meta(&c, "layout_version", "unsupported-layout").unwrap();
    drop(c);
    assert!(StagingRepo::open_partition_staging_conn(temp.path(), "source", 2).is_err());
    let c = Connection::open(&path).unwrap();
    assert_eq!(
        Repo::meta(&c, "layout_version").unwrap().as_deref(),
        Some("unsupported-layout")
    );
    assert_eq!(
        StagingRepo::get_staging_meta(&c, "status")
            .unwrap()
            .as_deref(),
        Some("done")
    );
    assert_eq!(
        c.query_row("SELECT name FROM ntfs_entries", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "root"
    );
}
#[test]
fn computed_null_parent_and_representative_do_not_mutate_raw_facts() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!(
        "../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
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
