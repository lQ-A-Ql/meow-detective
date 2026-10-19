use std::fs;
use tempfile::tempdir;

#[test]
fn reads_embedded_metadata_and_rejects_bad_footer() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("Meow Detective.exe");
    let metadata = br#"{"format":1,"version":"0.1.0","caseSchema":"schema"}"#;
    let mut bytes = vec![b'M', b'Z'];
    bytes.extend_from_slice(metadata);
    bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    bytes.extend_from_slice(portable_updater::METADATA_MAGIC);
    fs::write(&path, bytes).unwrap();
    assert_eq!(
        portable_updater::read_metadata(&path).unwrap().version,
        "0.1.0"
    );
    fs::OpenOptions::new().write(true).open(&path).unwrap();
    let mut corrupt = fs::read(&path).unwrap();
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(&path, corrupt).unwrap();
    assert!(portable_updater::read_metadata(&path).is_err());
}

#[test]
fn failed_boot_can_restore_the_exact_previous_executable() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    let target = root.join("Meow Detective.exe");
    let incoming = root.join("next.exe");
    fs::create_dir(root.join(".meow-update")).unwrap();
    fs::write(&target, b"old").unwrap();
    fs::write(&incoming, b"new").unwrap();
    portable_updater::apply(&target, &incoming).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert!(portable_updater::rollback_pending(root));
    portable_updater::recover(&target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert!(!portable_updater::rollback_pending(root));
}
