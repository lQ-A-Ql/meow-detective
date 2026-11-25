use super::*;
use persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo as Repo;

fn setup(map: &HashMap<u64, (Option<u64>, String, bool)>) -> rusqlite::Connection {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    Repo::begin(&c, "source", 4).unwrap();
    for (id, (parent, name, dir)) in map {
        c.execute("INSERT INTO ntfs_entries(record_num,parent_record_num,sequence_num,name,entry_type,deleted,hidden,system,read_only,archive) VALUES(?1,?2,7,?3,?4,0,0,0,0,0)",rusqlite::params![id,parent,name,i32::from(*dir)]).unwrap();
    }
    c
}

#[test]
fn parent_identity_rejects_reused_files_and_directory_sequences() {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let mut records = vec![
        fake_mft_record(5, 5, ".", true),
        fake_mft_record(36, 5, "thumbcache_idx.db", false),
        fake_deleted_mft_record(107154, 36, "Prefetch", true),
        fake_deleted_mft_record(131753, 107154, "TABTIP.pf", false),
        fake_mft_record(42, 5, "Users", true),
        fake_deleted_mft_record(43, 42, "NTUSER.DAT", false),
    ];
    records[1].sequence_number = 3;
    records[2].parent_sequence_number = 1;
    records[2].sequence_number = 2;
    records[3].parent_sequence_number = 1;
    records[4].sequence_number = 7;
    records[5].parent_sequence_number = 6;
    let mut catalog = stage_mft_records_for_test(&c, &records, "source", 4);
    catalog
        .update_staging_paths(&c, "source", 4, &AtomicBool::new(false))
        .unwrap();
    let path = |id| {
        c.query_row(
            "SELECT resolved_path FROM ntfs_paths WHERE record_num=?1",
            [id],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    };
    assert_eq!(path(107154), "/$DeletedOrphans/107154-Prefetch");
    assert_eq!(path(131753), "/$DeletedOrphans/107154-Prefetch/TABTIP.pf");
    assert_eq!(path(43), "/$DeletedOrphans/43-NTUSER.DAT");
    Repo::tree_shape(&c).unwrap();
}
#[test]
fn narrow_paths_keep_facts_and_exact_root_orphan_deleted_cycle_values() {
    let map = HashMap::from([
        (5, (None, "\\".into(), true)),
        (42, (Some(5), "Windows".into(), true)),
        (43, (Some(42), "a.txt".into(), false)),
        (100, (Some(999), "stray.txt".into(), false)),
        (101, (Some(999), "gone.txt".into(), false)),
        (200, (Some(201), "cycleA".into(), true)),
        (201, (Some(200), "cycleB".into(), true)),
    ]);
    let c = setup(&map);
    c.execute_batch("CREATE TRIGGER no_fact_path_update BEFORE UPDATE OF name,parent_record_num ON ntfs_entries BEGIN SELECT RAISE(ABORT,'fact mutation'); END").unwrap();
    let stats = update_mft_staging_paths_via_sqlite(
        &c,
        "source",
        4,
        &map,
        &HashSet::from([101]),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(stats.updated_main_rows, 0);
    assert_eq!(stats.materialized_rows, 7);
    let actual:Vec<(u64,String,Option<u64>,u8)>=c.prepare("SELECT record_num,resolved_path,catalog_parent_record_num,resolution_status FROM ntfs_paths ORDER BY record_num").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap().collect::<Result<_,_>>().unwrap();
    assert_eq!(
        actual,
        vec![
            (5, "".into(), None, 0),
            (42, "Windows".into(), Some(5), 1),
            (43, "Windows/a.txt".into(), Some(42), 1),
            (100, "/Unresolved/stray.txt".into(), Some(5), 3),
            (101, "/$DeletedOrphans/101-gone.txt".into(), Some(5), 2),
            (200, "/Unresolved/cycleB/cycleA".into(), Some(201), 3),
            (201, "/Unresolved/cycleB".into(), Some(200), 3)
        ]
    );
    assert_eq!(
        c.query_row(
            "SELECT parent_record_num FROM ntfs_entries WHERE record_num=100",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        999
    );
    assert!(validate_mft_staging_shape(&c, "source", 4).is_err()); // cycles must not become Ready
}
#[test]
fn cancellation_during_narrow_insert_restores_the_previous_projection_and_marker() {
    let map = HashMap::from([
        (5, (None, "\\".into(), true)),
        (16, (Some(5), "a".into(), false)),
        (17, (Some(5), "b".into(), false)),
    ]);
    let c = setup(&map);
    update_mft_staging_paths_via_sqlite(
        &c,
        "source",
        4,
        &map,
        &HashSet::new(),
        &AtomicBool::new(false),
    )
    .unwrap();
    Repo::tree_shape(&c).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    c.create_scalar_function(
        "cancel_path",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        move |_| {
            signal.store(true, Ordering::Release);
            Ok(0)
        },
    )
    .unwrap();
    c.execute_batch("CREATE TRIGGER cancel_insert AFTER INSERT ON ntfs_paths WHEN NEW.record_num=16 BEGIN SELECT cancel_path(); END").unwrap();
    assert!(matches!(
        update_mft_staging_paths_via_sqlite(&c, "source", 4, &map, &HashSet::new(), &cancel),
        Err(ParallelEnumError::Cancelled)
    ));
    assert!(c.is_autocommit());
    Repo::validate_paths(&c).unwrap();
    assert_eq!(Repo::tree_shape(&c).unwrap(), (3, 1, 0, 3));
    let name: String = c
        .query_row(
            "SELECT name FROM file_entries WHERE id='mft:4:17'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "b");
}
#[test]
fn very_deep_ancestor_chain_is_bounded_and_never_publishes_partial_paths() {
    let mut map = HashMap::from([(5, (None, "\\".into(), true))]);
    for id in 100..70_100 {
        map.insert(
            id,
            (
                Some(if id == 70_099 { 5 } else { id + 1 }),
                "x".into(),
                true,
            ),
        );
    }
    let c = setup(&map);
    let result = update_mft_staging_paths_via_sqlite(
        &c,
        "source",
        4,
        &map,
        &HashSet::new(),
        &AtomicBool::new(false),
    );
    assert!(matches!(
        result,
        Err(ParallelEnumError::MftResourceLimit(_))
    ));
    assert!(!result.err().unwrap().allows_fallback());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM ntfs_paths", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert!(Repo::validate_paths(&c).is_err());
}

#[test]
fn long_leaf_paths_stream_without_a_second_catalog_sized_path_cache() {
    let mut map = HashMap::from([(5, (None, "\\".into(), true))]);
    let long_name = "数据".repeat(100);
    for id in 100..2100 {
        map.insert(id, (Some(5), format!("{long_name}-{id}"), false));
    }
    let conn = setup(&map);
    let stats = update_mft_staging_paths_via_sqlite(
        &conn,
        "source",
        4,
        &map,
        &HashSet::new(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(stats.materialized_rows, 2001);
    assert_eq!(stats.cached_parent_paths, 1);
    assert_eq!(stats.cached_parent_bytes, 0);
    let last: String = conn
        .query_row(
            "SELECT resolved_path FROM ntfs_paths WHERE record_num=2099",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(last, format!("{long_name}-2099"));
    Repo::validate_paths(&conn).unwrap();
}
