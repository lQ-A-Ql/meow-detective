use persistence_fjall::{
    publish_build, seal_build, verify_sealed, DatabaseConfig, KeyPart, PersistMode,
    SingleWriterTxDatabase, SourceBuildLayout, WriteAction,
};
use std::{fs, path::PathBuf, process::Command};

fn workspace() -> PathBuf {
    let path = std::env::var_os("FJALL_PROCESS_RECOVERY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap()
                .join("target")
                .join("stage8-process-recovery")
        });
    if std::env::var_os("FJALL_PROCESS_RECOVERY_CHILD").is_none() {
        let _ = fs::remove_dir_all(&path);
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn child_process_termination_matrix_reopens_each_durable_boundary() {
    run_matrix_child_if_requested();
    for node in [
        "batch_commit",
        "sync_data",
        "sync_all",
        "seal",
        "rename",
        "source_generation_seal",
        "source_generation_rename",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(node);
        fs::create_dir_all(&path).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("child_process_termination_matrix_reopens_each_durable_boundary")
            .arg("--nocapture")
            .env("FJALL_PROCESS_RECOVERY_CHILD", "1")
            .env("FJALL_PROCESS_RECOVERY_NODE", node)
            .env(
                "FJALL_PROCESS_RECOVERY_PATH",
                path.to_string_lossy().to_string(),
            )
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(90), "node={node}");
        match node {
            "batch_commit" | "sync_data" | "sync_all" => {
                let database =
                    SingleWriterTxDatabase::open(&path, DatabaseConfig::default()).unwrap();
                let snapshot = database.read_snapshot();
                let key =
                    persistence_fjall::encode_key("matrix", &[KeyPart::Text(node.to_owned())])
                        .unwrap();
                assert_eq!(
                    snapshot
                        .get(persistence_fjall::KeyspaceName::Meta, &key)
                        .unwrap(),
                    Some(node.as_bytes().to_vec()),
                    "node={node}"
                );
            }
            "seal" => {
                let build = path
                    .join("sources")
                    .join("source-1")
                    .join("source.fjall.build.attempt");
                assert_eq!(verify_sealed(build).unwrap().len(), 64);
            }
            "rename" => {
                let published = path.join("sources").join("source-1").join("source.fjall");
                assert!(published.is_dir());
                assert_eq!(verify_sealed(published).unwrap().len(), 64);
            }
            "source_generation_seal" | "source_generation_rename" => {
                let layout =
                    SourceBuildLayout::for_case_source(&path, "source-1", "attempt").unwrap();
                let source_root = if node == "source_generation_seal" {
                    layout.build_path()
                } else {
                    layout.published_path()
                };
                let source =
                    persistence_fjall::SourceStore::open(source_root, DatabaseConfig::default())
                        .unwrap();
                let manifest = persistence_fjall::verify_source_generation_with_control(
                    &source,
                    "source-1",
                    &std::sync::atomic::AtomicBool::new(false),
                    |_, _| {},
                )
                .unwrap();
                assert_eq!(manifest.attempt_id, "attempt");
                assert_eq!(manifest.projection_batch_digest.len(), 64);
            }
            _ => unreachable!(),
        }
    }
}

fn run_matrix_child_if_requested() {
    if std::env::var_os("FJALL_PROCESS_RECOVERY_CHILD").is_none() {
        return;
    }
    let node = std::env::var("FJALL_PROCESS_RECOVERY_NODE").unwrap();
    let path = PathBuf::from(std::env::var("FJALL_PROCESS_RECOVERY_PATH").unwrap());
    match node.as_str() {
        "batch_commit" => {
            let database = SingleWriterTxDatabase::open(&path, DatabaseConfig::default()).unwrap();
            database
                .write_batch(
                    &[WriteAction::Put {
                        keyspace: persistence_fjall::KeyspaceName::Meta,
                        key: persistence_fjall::encode_key(
                            "matrix",
                            &[KeyPart::Text(node.clone())],
                        )
                        .unwrap(),
                        value: node.as_bytes().to_vec(),
                    }],
                    PersistMode::Buffer,
                )
                .unwrap();
        }
        "sync_data" | "sync_all" => {
            let database = SingleWriterTxDatabase::open(&path, DatabaseConfig::default()).unwrap();
            database
                .write_batch(
                    &[WriteAction::Put {
                        keyspace: persistence_fjall::KeyspaceName::Meta,
                        key: persistence_fjall::encode_key(
                            "matrix",
                            &[KeyPart::Text(node.clone())],
                        )
                        .unwrap(),
                        value: node.as_bytes().to_vec(),
                    }],
                    PersistMode::Buffer,
                )
                .unwrap();
            database
                .persist(if node == "sync_data" {
                    PersistMode::SyncData
                } else {
                    PersistMode::SyncAll
                })
                .unwrap();
        }
        "seal" | "rename" => {
            let layout = SourceBuildLayout::for_case_source(&path, "source-1", "attempt").unwrap();
            fs::create_dir_all(layout.build_path()).unwrap();
            fs::write(layout.build_path().join("payload"), node.as_bytes()).unwrap();
            seal_build(layout.build_path()).unwrap();
            if node == "rename" {
                publish_build(layout.build_path(), layout.published_path()).unwrap();
            }
        }
        "source_generation_seal" | "source_generation_rename" => {
            let layout = SourceBuildLayout::for_case_source(&path, "source-1", "attempt").unwrap();
            fs::create_dir_all(layout.build_path()).unwrap();
            let source = persistence_fjall::SourceStore::open(
                layout.build_path(),
                DatabaseConfig::default(),
            )
            .unwrap();
            let receipt = source
                .database()
                .write_batch(
                    &[WriteAction::Put {
                        keyspace: persistence_fjall::KeyspaceName::Meta,
                        key: b"projection".to_vec(),
                        value: b"complete".to_vec(),
                    }],
                    PersistMode::SyncAll,
                )
                .unwrap();
            persistence_fjall::seal_source_generation_with_control(
                &source,
                "source-1",
                "attempt",
                &hex::encode(receipt.batch_digest),
                &std::sync::atomic::AtomicBool::new(false),
                |_, _| {},
            )
            .unwrap();
            drop(source);
            if node == "source_generation_rename" {
                persistence_fjall::publish_source_generation(
                    layout.build_path(),
                    layout.published_path(),
                )
                .unwrap();
            }
        }
        _ => panic!("unknown recovery matrix node: {node}"),
    }
    std::process::exit(90);
}

#[test]
fn child_process_terminates_after_batch_and_parent_reopens_receipt() {
    let path = workspace();
    if std::env::var_os("FJALL_PROCESS_RECOVERY_CHILD").is_some() {
        let database = SingleWriterTxDatabase::open(&path, DatabaseConfig::default()).unwrap();
        database
            .write_batch(
                &[WriteAction::Put {
                    keyspace: persistence_fjall::KeyspaceName::Meta,
                    key: persistence_fjall::encode_key("process", &[KeyPart::Text("batch".into())])
                        .unwrap(),
                    value: b"committed-before-exit".to_vec(),
                }],
                PersistMode::SyncAll,
            )
            .unwrap();
        std::process::exit(99);
    }
    let status = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("child_process_terminates_after_batch_and_parent_reopens_receipt")
        .arg("--nocapture")
        .env("FJALL_PROCESS_RECOVERY_CHILD", "1")
        .env(
            "FJALL_PROCESS_RECOVERY_PATH",
            path.to_string_lossy().to_string(),
        )
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(99));
    let database = SingleWriterTxDatabase::open(&path, DatabaseConfig::default()).unwrap();
    let snapshot = database.read_snapshot();
    let key = persistence_fjall::encode_key("process", &[KeyPart::Text("batch".into())]).unwrap();
    assert_eq!(
        snapshot
            .get(persistence_fjall::KeyspaceName::Meta, &key)
            .unwrap(),
        Some(b"committed-before-exit".to_vec())
    );
}
