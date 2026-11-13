use domain::DataSourceId;
use persistence_source::{
    control, detail,
    fjall::{
        clone_verified_generation_with_control, seal_source_generation_with_control,
        DatabaseConfig, SourceStore,
    },
    publication::{
        drain_analysis_generation_gc, reclaim_analysis_generations, request_analysis_generation_gc,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

fn fixture() -> (tempfile::TempDir, PathBuf, SourceStore) {
    let scratch =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/analysis/tests");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let case = root.path().join("case");
    let connection = control::open_source_db(&case, &DataSourceId("source".into())).unwrap();
    let source = detail::open_store_for_write(&connection).unwrap().unwrap();
    let receipt = source
        .write_raw(
            persistence_source::fjall::KeyspaceName::Meta,
            b"fixture".to_vec(),
            b"gc".to_vec(),
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .unwrap();
    seal_source_generation_with_control(
        &source,
        "source",
        "original",
        &hex::encode(receipt.batch_digest),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    (root, case, (*source).clone())
}

fn generation(source: &SourceStore) -> PathBuf {
    let attempt = format!("analysis-{}", uuid::Uuid::new_v4());
    let path = source
        .database()
        .path()
        .parent()
        .unwrap()
        .join(format!("source.fjall.build.{attempt}"));
    let (target, verified) = clone_verified_generation_with_control(
        source,
        "source",
        &path,
        DatabaseConfig::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    verified
        .seal(&target, &attempt, &AtomicBool::new(false))
        .unwrap();
    path
}

#[test]
fn reclamation_is_bounded_and_preserves_unknown_directories() {
    let (_root, _case, source) = fixture();
    let directory = source.database().path().parent().unwrap();
    let paths = (0..10).map(|_| generation(&source)).collect::<Vec<_>>();
    let unknown = directory.join(format!(
        "source.fjall.build.analysis-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&unknown).unwrap();
    assert_eq!(
        reclaim_analysis_generations(directory)
            .unwrap()
            .unwrap()
            .removed,
        8
    );
    assert_eq!(
        reclaim_analysis_generations(directory)
            .unwrap()
            .unwrap()
            .removed,
        2
    );
    assert!(paths.iter().all(|path| !path.exists()));
    assert!(unknown.is_dir());
    assert!(source.database().path().is_dir());
}

#[test]
fn an_old_sqlite_transaction_blocks_gc_before_its_first_generation_resolution() {
    let (_root, case, source) = fixture();
    let source_id = DataSourceId("source".into());
    let old = generation(&source);
    let new = generation(&source);
    let reader = control::open_source_db(&case, &source_id).unwrap();
    detail::bind_generation(&reader, old.file_name().unwrap().to_str().unwrap()).unwrap();
    let transaction = reader.unchecked_transaction().unwrap();
    let observed: String = transaction
        .query_row(
            "SELECT value FROM source_meta WHERE key='fjall_root'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let writer = control::open_source_db(&case, &source_id).unwrap();
    detail::bind_generation(&writer, new.file_name().unwrap().to_str().unwrap()).unwrap();
    drop(writer);
    assert!(
        reclaim_analysis_generations(source.database().path().parent().unwrap())
            .unwrap()
            .is_none()
    );
    let retained: String = transaction
        .query_row(
            "SELECT value FROM source_meta WHERE key='fjall_root'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(observed, retained);
    assert!(old.is_dir());
    drop(transaction);
    drop(reader);
    assert_eq!(
        reclaim_analysis_generations(source.database().path().parent().unwrap())
            .unwrap()
            .unwrap()
            .removed,
        1
    );
    assert!(!old.exists());
    assert!(new.is_dir());
}

#[test]
fn queued_gc_completes_after_last_sql_and_fjall_references_close() {
    let (_root, case, source) = fixture();
    let directory = source.database().path().parent().unwrap();
    let old = generation(&source);
    let held = persistence_source::fjall::open_shared_source_store(&old, DatabaseConfig::default())
        .unwrap();
    let connection = control::open_source_db(&case, &DataSourceId("source".into())).unwrap();
    request_analysis_generation_gc(directory).unwrap();
    assert!(old.is_dir());
    drop(connection);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        old.is_dir(),
        "live Fjall reference must retain the generation"
    );
    drop(held);
    drain_analysis_generation_gc(Duration::from_secs(5)).unwrap();
    assert!(!old.exists());
    assert!(source.database().path().is_dir());
}

#[test]
fn a_new_process_can_reclaim_generations_without_a_previous_memory_queue() {
    let (_root, _case, source) = fixture();
    let old = generation(&source);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "gc_after_restart_child",
            "--nocapture",
        ])
        .env(
            "FORENSICS_GC_RESTART_SOURCE",
            source.database().path().parent().unwrap(),
        )
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!old.exists());
    assert!(source.database().path().is_dir());
}

#[test]
#[ignore = "requires FORENSICS_GC_RESTART_SOURCE from the parent regression test"]
fn gc_after_restart_child() {
    let directory = PathBuf::from(
        std::env::var_os("FORENSICS_GC_RESTART_SOURCE")
            .expect("parent test supplies source directory"),
    );
    request_analysis_generation_gc(&directory).unwrap();
    drain_analysis_generation_gc(Duration::from_secs(5)).unwrap();
}
