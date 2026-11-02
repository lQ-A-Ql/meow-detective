use persistence_fjall::{
    DatabaseConfig, KeyPart, PersistMode, SingleWriterTxDatabase, WriteAction,
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
