use super::*;
use persistence_fjall::{
    clone_verified_generation_with_control, seal_source_generation_with_control, DatabaseConfig,
    KeyspaceName, PersistMode, SourceStore, SOURCE_GENERATION_MANIFEST_FILE,
    SOURCE_GENERATION_SEAL_FILE,
};
use std::sync::atomic::AtomicBool;

fn fixture() -> (tempfile::TempDir, PathBuf, SourceStore) {
    let scratch =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/analysis/tests");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let case = root.path().join("case");
    let connection = crate::control::open_source_db(&case, &DataSourceId("source".into())).unwrap();
    let source = crate::detail::open_store_for_write(&connection)
        .unwrap()
        .unwrap();
    let receipt = source
        .write_raw(
            KeyspaceName::Meta,
            b"fixture".to_vec(),
            b"intent".to_vec(),
            PersistMode::SyncAll,
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
    (
        root,
        case.join("sources/source").canonicalize().unwrap(),
        (*source).clone(),
    )
}

fn old_generation(source: &SourceStore) -> PathBuf {
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
fn persisted_authorization_resumes_after_real_partial_directory_removal() {
    let (_root, directory, source) = fixture();
    let old = old_generation(&source);
    let guard = try_begin_generation_gc(&directory).unwrap().unwrap();
    let intent = retired::stage(
        &guard,
        &directory,
        source.database().path(),
        &old,
        &DataSourceId("source".into()),
    )
    .unwrap()
    .unwrap();
    let quarantine = intent.quarantine(&directory);
    let intent_path = intent.path.clone();
    assert!(!old.exists());
    assert!(intent_path.is_file());
    std::fs::remove_file(quarantine.join(SOURCE_GENERATION_MANIFEST_FILE)).unwrap();
    std::fs::remove_file(quarantine.join(SOURCE_GENERATION_SEAL_FILE)).unwrap();
    drop(intent);
    drop(guard);
    assert!(has_pending_analysis_gc(&directory).unwrap());
    assert_eq!(
        reclaim_analysis_generations(&directory)
            .unwrap()
            .unwrap()
            .removed,
        1
    );
    assert!(!quarantine.exists());
    assert!(!intent_path.exists());
    assert!(!has_pending_analysis_gc(&directory).unwrap());
    assert!(source
        .database()
        .path()
        .join(SOURCE_GENERATION_SEAL_FILE)
        .exists());
}

#[test]
fn authorization_never_deletes_a_replacement_or_the_current_directory() {
    let (_root, directory, source) = fixture();
    let old = old_generation(&source);
    let guard = try_begin_generation_gc(&directory).unwrap().unwrap();
    assert!(retired::stage(
        &guard,
        &directory,
        source.database().path(),
        source.database().path(),
        &DataSourceId("source".into())
    )
    .is_err());
    let intent = retired::stage(
        &guard,
        &directory,
        source.database().path(),
        &old,
        &DataSourceId("source".into()),
    )
    .unwrap()
    .unwrap();
    let quarantine = intent.quarantine(&directory);
    let preserved = directory.join("preserved-authorized-directory");
    std::fs::rename(&quarantine, &preserved).unwrap();
    std::fs::create_dir(&quarantine).unwrap();
    std::fs::write(
        quarantine.join("unknown.txt"),
        b"preserve unknown replacement",
    )
    .unwrap();
    drop(intent);
    drop(guard);
    assert!(reclaim_analysis_generations(&directory).is_err());
    assert_eq!(
        std::fs::read(quarantine.join("unknown.txt")).unwrap(),
        b"preserve unknown replacement"
    );
    assert!(preserved.is_dir());
    assert!(source.database().path().is_dir());
}

#[test]
fn an_unknown_quarantine_without_authorization_is_preserved() {
    let (_root, directory, _source) = fixture();
    let unknown = directory.join(format!(".analysis-gc-{}.removing", uuid::Uuid::new_v4()));
    std::fs::create_dir(&unknown).unwrap();
    std::fs::write(unknown.join("unknown.txt"), b"unrecognized directory").unwrap();
    assert!(!has_pending_analysis_gc(&directory).unwrap());
    assert_eq!(
        reclaim_analysis_generations(&directory)
            .unwrap()
            .unwrap()
            .removed,
        0
    );
    assert!(unknown.is_dir());
}

#[test]
fn a_modified_foreign_intent_cannot_authorize_directory_deletion() {
    let (_root, directory, source) = fixture();
    let old = old_generation(&source);
    let guard = try_begin_generation_gc(&directory).unwrap().unwrap();
    let intent = retired::stage(
        &guard,
        &directory,
        source.database().path(),
        &old,
        &DataSourceId("source".into()),
    )
    .unwrap()
    .unwrap();
    let quarantine = intent.quarantine(&directory);
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&intent.path).unwrap()).unwrap();
    record["sourceId"] = "another-source".into();
    std::fs::write(&intent.path, serde_json::to_vec(&record).unwrap()).unwrap();
    drop(intent);
    drop(guard);
    assert!(!has_pending_analysis_gc(&directory).unwrap());
    assert_eq!(
        reclaim_analysis_generations(&directory)
            .unwrap()
            .unwrap()
            .removed,
        0
    );
    assert!(quarantine.is_dir());
}
