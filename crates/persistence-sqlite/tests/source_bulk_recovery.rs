use persistence_sqlite::repositories::source_bulk_load_repo::{
    BulkLoadState, SourceBulkLoadRepo as Repo,
};
use rusqlite::Connection;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn indexes(c: &Connection) -> Vec<(String, Option<String>)> {
    c.prepare("SELECT name,sql FROM sqlite_master WHERE type='index' AND tbl_name='file_entries' ORDER BY name").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap()
}
fn definitions(c: &Connection) -> Vec<(String, String)> {
    let text: String = c
        .query_row(
            "SELECT value FROM source_meta WHERE key='bulk_load.indexes'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn every_durable_state_reopens_and_recovers_exact_index_sql() {
    for state in [
        BulkLoadState::Active,
        BulkLoadState::Rebuilding,
        BulkLoadState::Ready,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("source.db");
        let c = persistence_sqlite::open_or_create_source(&path).unwrap();
        c.execute_batch("CREATE UNIQUE INDEX preserved_unique ON file_entries(path); CREATE INDEX \"quoted index\" ON file_entries(lower(name))").unwrap();
        let original = indexes(&c);
        assert!(Repo::begin(&c, "attempt").unwrap());
        assert!(indexes(&c).iter().any(|(n, _)| n == "preserved_unique"));
        let saved = definitions(&c);
        assert!(!saved.is_empty());
        if state != BulkLoadState::Active {
            c.execute(&saved[0].1, []).unwrap();
        }
        Repo::set_state(&c, state, "attempt").unwrap();
        drop(c);
        let c = persistence_sqlite::open_existing_source(&path).unwrap();
        assert_eq!(Repo::state(&c).unwrap(), state);
        Repo::recover(&c).unwrap();
        assert_eq!(indexes(&c), original);
        assert_eq!(Repo::state(&c).unwrap(), BulkLoadState::Inactive);
        Repo::recover(&c).unwrap();
        assert_eq!(indexes(&c), original);
    }
}
#[test]
fn nonempty_source_keeps_all_indexes_and_active_load_keeps_unique_constraints() {
    let temp = tempfile::tempdir().unwrap();
    let c = persistence_sqlite::open_or_create_source(&temp.path().join("source.db")).unwrap();
    c.execute_batch("CREATE UNIQUE INDEX preserved_unique ON file_entries(path)")
        .unwrap();
    let original = indexes(&c);
    assert!(Repo::begin(&c, "fresh").unwrap());
    c.execute_batch("INSERT INTO file_entries(id,data_source_id,path,name,entry_type) VALUES('a','source','same','a','file')").unwrap();
    assert!(c.execute_batch("INSERT INTO file_entries(id,data_source_id,path,name,entry_type) VALUES('b','source','same','b','file')").is_err());
    Repo::finish(&c).unwrap();
    assert_eq!(indexes(&c), original);
    assert!(!Repo::begin(&c, "nonempty").unwrap());
    assert_eq!(indexes(&c), original);
}
#[test]
fn failed_ready_cleanup_keeps_manifest_for_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let c = persistence_sqlite::open_or_create_source(&temp.path().join("source.db")).unwrap();
    let original = indexes(&c);
    Repo::begin(&c, "cleanup").unwrap();
    c.execute_batch("CREATE TRIGGER fail_cleanup BEFORE DELETE ON source_meta WHEN OLD.key='bulk_load.indexes' BEGIN SELECT RAISE(ABORT,'cleanup failure'); END").unwrap();
    assert!(Repo::finish(&c).is_err());
    assert_eq!(Repo::state(&c).unwrap(), BulkLoadState::Ready);
    assert_eq!(indexes(&c), original);
    assert!(!definitions(&c).is_empty());
    c.execute_batch("DROP TRIGGER fail_cleanup").unwrap();
    Repo::recover(&c).unwrap();
    assert_eq!(Repo::state(&c).unwrap(), BulkLoadState::Inactive);
}

#[test]
fn orphaned_manifest_never_looks_inactive_or_ready() {
    let temp = tempfile::tempdir().unwrap();
    let c = persistence_sqlite::open_or_create_source(&temp.path().join("source.db")).unwrap();
    c.execute(
        "INSERT INTO source_meta(key,value) VALUES('bulk_load.indexes','[]')",
        [],
    )
    .unwrap();
    assert!(Repo::state(&c).is_err());
    assert!(Repo::recover(&c).is_err());
    assert_eq!(
        c.query_row(
            "SELECT value FROM source_meta WHERE key='bulk_load.indexes'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "[]"
    );
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn killed_rebuild_rolls_back_index_ddl_and_recovers_from_durable_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.db");
    let ready = temp.path().join("ready");
    let c = persistence_sqlite::open_or_create_source(&path).unwrap();
    let original = indexes(&c);
    Repo::begin(&c, "crash").unwrap();
    let deferred = indexes(&c);
    drop(c);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "bulk_rebuild_crash_child",
            "--exact",
            "--ignored",
            "--test-threads=1",
        ])
        .env("FORENSICS_S2_BULK_DB", &path)
        .env("FORENSICS_S2_BULK_READY", &ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = ChildGuard(command.spawn().unwrap());
    let start = Instant::now();
    while !ready.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "rebuild child did not reach DDL"
        );
        assert!(child.0.try_wait().unwrap().is_none(), "child exited early");
        std::thread::sleep(Duration::from_millis(10));
    }
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let c = persistence_sqlite::open_existing_source(&path).unwrap();
    assert_eq!(indexes(&c), deferred);
    assert_eq!(Repo::state(&c).unwrap(), BulkLoadState::Rebuilding);
    Repo::recover(&c).unwrap();
    assert_eq!(indexes(&c), original);
}
#[test]
#[ignore = "child helper for owned temporary source database crash test"]
fn bulk_rebuild_crash_child() {
    let path = std::env::var_os("FORENSICS_S2_BULK_DB").expect("child database");
    let ready = std::env::var_os("FORENSICS_S2_BULK_READY").expect("child signal");
    let c = persistence_sqlite::open_existing_source(std::path::Path::new(&path)).unwrap();
    Repo::set_state(&c, BulkLoadState::Rebuilding, "crash").unwrap();
    let saved = definitions(&c);
    c.execute_batch("PRAGMA cache_size=1; PRAGMA cache_spill=ON; BEGIN IMMEDIATE")
        .unwrap();
    for (_, sql) in saved.iter().take(3) {
        c.execute(sql, []).unwrap();
    }
    std::fs::write(ready, b"uncommitted DDL").unwrap();
    loop {
        std::thread::park_timeout(Duration::from_secs(60));
    }
}
