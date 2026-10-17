use persistence_sqlite::repositories::{
    ntfs_staging_repo::NtfsStagingRepo as Repo, source_bulk_load_repo::SourceBulkLoadRepo,
    staging_repo::StagingRepo,
};
use rusqlite::{types::Value, Connection};

fn seed(c: &Connection, name: &str) {
    Repo::begin(c, "source", 1).unwrap();
    for (id, parent, name, dir) in [
        (5, None, name, 1),
        (16, Some(5), "SHORT.TXT", 0),
        (17, Some(5), "Dir", 1),
        (18, Some(17), "f.bin", 0),
    ] {
        c.execute("INSERT INTO ntfs_entries(record_num,parent_record_num,sequence_num,name,entry_type,size,ext,deleted,hidden,system,read_only,archive,encrypted,created_at,modified_at,accessed_at,changed_at) VALUES(?1,?2,7,?3,?4,12,'txt',0,1,1,1,1,NULL,'2020-01-01',NULL,NULL,NULL)",rusqlite::params![id,parent,name,dir]).unwrap();
    }
    Repo::write_paths(c, |s| -> rusqlite::Result<()> {
        for (id, path, name, parent) in [
            (5, "", name, None),
            (16, "Long.exe", "Long.exe", Some(5)),
            (17, "Dir", "Dir", Some(5)),
            (18, "Dir/f.bin", "f.bin", Some(17)),
        ] {
            s.execute(rusqlite::params![
                id,
                path,
                name,
                parent,
                if id == 5 { 0 } else { 1 }
            ])?;
        }
        Ok(())
    })
    .unwrap();
    Repo::validate_paths(c).unwrap();
    assert_eq!(Repo::tree_shape(c).unwrap(), (4, 1, 0, 4));
    StagingRepo::complete_enum(c, 2, 2).unwrap();
    StagingRepo::set_staging_meta(c, "enum_strategy", "mft").unwrap();
}
fn snapshot(c: &Connection) -> Vec<Vec<Value>> {
    let root: String = c
        .query_row(
            "SELECT id FROM file_entries WHERE parent_id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut q = c
        .prepare("SELECT * FROM file_entries ORDER BY path,name,id")
        .unwrap();
    let n = q.column_count();
    q.query_map([], |r| {
        (0..n)
            .map(|i| {
                let v: Value = r.get(i)?;
                Ok(if let Value::Text(s) = v {
                    Value::Text(s.replace(&root, "__root__"))
                } else {
                    v
                })
            })
            .collect::<Result<Vec<_>, _>>()
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}
#[test]
fn completed_numeric_merge_matches_compatibility_view_for_all_root_names() {
    for root_name in ["\\", "/", ".", "RootName"] {
        let temp = tempfile::tempdir().unwrap();
        let staging = StagingRepo::open_partition_staging_conn(temp.path(), "source", 1).unwrap();
        seed(&staging, root_name);
        let legacy =
            persistence_sqlite::open_or_create_source(&temp.path().join("old.db")).unwrap();
        let numeric =
            persistence_sqlite::open_or_create_source(&temp.path().join("new.db")).unwrap();
        staging
            .execute(
                "DELETE FROM ntfs_staging_meta WHERE key='tree_validated'",
                [],
            )
            .unwrap();
        let a = StagingRepo::merge_enum_staging_to_main(&legacy, &staging, "source", 1, "NTFS")
            .unwrap();
        Repo::tree_shape(&staging).unwrap();
        let b = StagingRepo::merge_enum_staging_to_main(&numeric, &staging, "source", 1, "NTFS")
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(snapshot(&legacy), snapshot(&numeric));
    }
}
#[test]
fn current_full_schema_plans_use_integer_lookups_and_covering_parent_index() {
    let temp = tempfile::tempdir().unwrap();
    let s = StagingRepo::open_partition_staging_conn(temp.path(), "source", 1).unwrap();
    seed(&s, "\\");
    let main = persistence_sqlite::open_or_create_source(&temp.path().join("source.db")).unwrap();
    main.execute("ATTACH DATABASE ?1 AS staging", [s.path().unwrap()])
        .unwrap();
    for bulk in [false, true] {
        if bulk {
            SourceBulkLoadRepo::begin(&main, "plan").unwrap();
        }
        let plans: Vec<String> = main
            .prepare(&format!(
                "EXPLAIN QUERY PLAN {}",
                include_str!("../src/repositories/ntfs_staging/merge.sql")
            ))
            .unwrap()
            .query_map(rusqlite::params!["root", 1], |r| r.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!plans.iter().any(|s| s.contains("CORRELATED")), "{plans:?}");
        assert!(
            plans.iter().any(|s| s.contains("INTEGER PRIMARY KEY")),
            "{plans:?}"
        );
        eprintln!("[numeric-merge-plan] bulk={bulk} {plans:?}");
    }
    SourceBulkLoadRepo::finish(&main).unwrap();
    main.execute_batch("DETACH DATABASE staging").unwrap();
    let plans: Vec<String> = s
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            include_str!("../src/repositories/ntfs_staging/tree_shape.sql")
        ))
        .unwrap()
        .query_map([], |r| r.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plans
            .iter()
            .any(|s| s.contains("COVERING INDEX ntfs_parent")),
        "{plans:?}"
    );
    assert!(
        plans.iter().any(|s| s.contains("INTEGER PRIMARY KEY")),
        "{plans:?}"
    );
    eprintln!("[numeric-tree-plan] {plans:?}");
}
#[test]
fn incomplete_paths_and_wrong_partition_cannot_publish_source_rows() {
    let temp = tempfile::tempdir().unwrap();
    let s = StagingRepo::open_partition_staging_conn(temp.path(), "source", 1).unwrap();
    seed(&s, "\\");
    let main = persistence_sqlite::open_or_create_source(&temp.path().join("source.db")).unwrap();
    assert!(StagingRepo::merge_enum_staging_to_main(&main, &s, "source", 9, "NTFS").is_err());
    assert_eq!(
        main.query_row("SELECT COUNT(*) FROM file_entries", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    s.execute("DELETE FROM ntfs_paths WHERE record_num=16", [])
        .unwrap();
    assert!(StagingRepo::merge_enum_staging_to_main(&main, &s, "source", 1, "NTFS").is_err());
    assert_eq!(
        main.query_row("SELECT COUNT(*) FROM file_entries", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
}
