use containers_bolt::{read_bolt, BoltError, BoltReadOptions};
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/go-bbolt.db")
}

fn options() -> BoltReadOptions {
    BoltReadOptions {
        prefix: Vec::new(),
        record_keys: Vec::new(),
        value_keys: vec![b"foo".to_vec()],
        parent_depth: None,
        bucket_parent_depth: None,
        parent_suffix: Vec::new(),
        include_buckets: true,
        max_file_bytes: 1024 * 1024,
        max_entries: 4096,
        max_depth: 32,
        max_output_bytes: 1024 * 1024,
    }
}

#[test]
fn go_database_preserves_inline_branch_binary_keys_and_read_only_bytes() {
    let before = std::fs::read(fixture()).unwrap();
    let report = read_bolt(&fixture(), &options()).unwrap();
    assert_eq!(report.metadata.txid, 3);
    assert_eq!(report.metadata.page_size, 4096);
    assert!(report.metadata.metadata_checksum_verified);
    assert!(report.entries.iter().any(|entry| entry.key == b"key-2047"));
    assert!(report
        .entries
        .iter()
        .any(|entry| entry.key == [0, 255, 1] && entry.value_length == Some(0)));
    let foo = report
        .entries
        .iter()
        .find(|entry| entry.key == b"foo")
        .unwrap();
    assert_eq!(foo.value.as_deref(), Some(b"new-value".as_slice()));
    assert_eq!(std::fs::read(fixture()).unwrap(), before);
}

#[test]
fn metadata_checksum_selects_valid_transaction_and_rejects_both_invalid_pages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checksum.db");
    let mut bytes = std::fs::read(fixture()).unwrap();
    // Corrupt only the checksum of metadata page 1 (transaction 3).
    bytes[4096 + 72] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    let report = read_bolt(&path, &options()).unwrap();
    assert_eq!(report.metadata.txid, 2);
    assert_eq!(
        report
            .entries
            .iter()
            .find(|entry| entry.key == b"foo")
            .unwrap()
            .value
            .as_deref(),
        Some(b"old-value".as_slice())
    );
    bytes[72] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    assert!(read_bolt(&path, &options()).is_err());
}

#[test]
fn selector_excludes_other_namespaces_and_limits_bound_visits_and_output() {
    let mut options = options();
    options.prefix = vec![b"v1".to_vec(), b"moby".to_vec(), b"images".to_vec()];
    options.parent_depth = Some(2);
    options.bucket_parent_depth = Some(0);
    options.parent_suffix = vec![b"target".to_vec()];
    options.record_keys = vec![b"digest".to_vec()];
    options.value_keys = options.record_keys.clone();
    let report = read_bolt(&fixture(), &options).unwrap();
    assert_eq!(report.entries.len(), 4);
    assert!(report.visited_entries < 12);
    assert!(report
        .entries
        .iter()
        .all(|entry| entry.bucket_path.starts_with(&options.prefix)));
    options.max_entries = 1;
    assert!(matches!(
        read_bolt(&fixture(), &options),
        Err(BoltError::EntryLimit)
    ));
    options.max_entries = 4096;
    options.max_output_bytes = 1;
    assert!(matches!(
        read_bolt(&fixture(), &options),
        Err(BoltError::OutputLimit)
    ));
    options.max_output_bytes = 1024 * 1024;
    options.max_file_bytes = 1;
    assert!(matches!(
        read_bolt(&fixture(), &options),
        Err(BoltError::FileLimit)
    ));
}
