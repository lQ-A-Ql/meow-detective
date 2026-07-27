use super::*;
use domain::FileEntryId;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use tempfile::TempDir;

struct MemoryEvidence {
    cursor: Cursor<Vec<u8>>,
    info: evidence_core::ReaderInfo,
}

impl MemoryEvidence {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            info: evidence_core::ReaderInfo {
                path: PathBuf::from("memory.raw"),
                size: bytes.len() as u64,
                kind: "memory".to_string(),
            },
            cursor: Cursor::new(bytes),
        }
    }
}

impl Read for MemoryEvidence {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(bytes)
    }
}

impl Seek for MemoryEvidence {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(position)
    }
}

impl evidence_core::EvidenceReader for MemoryEvidence {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

#[test]
fn descriptor_cache_is_bounded_for_large_analysis_scans() {
    let mut cache = HashMap::new();
    for index in 0..=MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES {
        cache_preview_descriptor(
            &mut cache,
            &format!("file-{index}"),
            &serde_json::json!({"fileId": index}),
        );
    }

    assert_eq!(cache.len(), 1);
    assert!(cache.contains_key(&format!("file-{MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES}")));
}

fn insert_source(
    connection: &rusqlite::Connection,
    source_id: &str,
    kind: &str,
    source_path: &str,
) {
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, 'case-1', 'source', ?2, ?3, '2026-07-18T00:00:00Z')",
            rusqlite::params![source_id, kind, source_path],
        )
        .expect("insert source");
}

fn source_read_hint(
    source_id: &str,
    file_id: &str,
    partition_index: Option<usize>,
    path: &str,
) -> SourceReadFileHint {
    SourceReadFileHint::new(
        FileEntryId(file_id.to_string()),
        DataSourceId(source_id.to_string()),
        partition_index,
        path.to_string(),
        16,
        false,
    )
}

#[test]
fn partition_metadata_fast_path_builds_and_caches_descriptor_without_file_entry_lookup() {
    let source_conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&source_conn).expect("run source migrations");
    insert_source(&source_conn, "derived-linux", "ceph_rbd", "derived.rbd");
    source_conn
        .execute(
            "INSERT INTO data_source_partitions
             (id, data_source_id, partition_index, name, kind_label, status,
              offset, length, filesystem)
             VALUES ('partition-2', 'derived-linux', 2, 'Root', 'XFS', 'ready',
                     0, 4096, 'XFS')",
            [],
        )
        .expect("insert partition");
    let case_conn = rusqlite::Connection::open_in_memory().expect("open case database");
    let case_root = TempDir::new().expect("create case root");
    let case_id = CaseId("case-1".to_string());
    let source_id = DataSourceId("derived-linux".to_string());
    let mut context = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_id,
    );

    let first = context
        .descriptor_for_hint(
            &source_read_hint(
                "derived-linux",
                "not-present-in-file-entries",
                Some(2),
                "var/www/a.php",
            ),
            2,
            "ceph_rbd".to_string(),
            "derived.rbd".to_string(),
        )
        .expect("build direct descriptor");
    source_conn
        .execute("DELETE FROM data_source_partitions", [])
        .expect("remove persisted partitions after cache fill");
    source_conn
        .execute("DELETE FROM data_sources", [])
        .expect("remove source location after cache fill");
    let second = context
        .descriptor_for_hint(
            &source_read_hint(
                "derived-linux",
                "also-not-present-in-file-entries",
                Some(2),
                "var/www/b.php",
            ),
            2,
            "ceph_rbd".to_string(),
            "derived.rbd".to_string(),
        )
        .expect("reuse source-level metadata caches");

    assert_eq!(first.partition_candidates.len(), 1);
    assert_eq!(first.partition_candidates[0].partition_index, 2);
    assert_eq!(second.partition_candidates, first.partition_candidates);
}

#[test]
fn null_partition_candidate_falls_back_to_existing_file_id_reader() {
    let source_conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&source_conn).expect("run source migrations");
    let evidence = TempDir::new().expect("create logical evidence");
    std::fs::write(evidence.path().join("fallback.txt"), b"fallback-by-id")
        .expect("write logical evidence");
    insert_source(
        &source_conn,
        "logical-linux",
        "logical_directory",
        evidence.path().to_string_lossy().as_ref(),
    );
    source_conn
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, deleted, hidden, system,
              encrypted, partition_index)
             VALUES ('fallback-file', 'logical-linux', 'fallback.txt', 'fallback.txt',
                     'file', 14, 0, 0, 0, 0, NULL)",
            [],
        )
        .expect("insert fallback file entry");
    let case_conn = rusqlite::Connection::open_in_memory().expect("open case database");
    let case_root = TempDir::new().expect("create case root");
    let case_id = CaseId("case-1".to_string());
    let source_id = DataSourceId("logical-linux".to_string());
    let mut context = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_id,
    );

    let bytes = context
        .read_file_header_with_metadata(
            SourceReadFileHint::new(
                FileEntryId("fallback-file".to_string()),
                DataSourceId("logical-linux".to_string()),
                None,
                "ignored-candidate-path".to_string(),
                16,
                false,
            ),
            64,
        )
        .expect("read through file-id fallback");

    assert_eq!(bytes, b"fallback-by-id");
}

#[test]
fn encrypted_metadata_hint_is_rejected_before_derived_runtime_initialization() {
    let source_conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&source_conn).expect("run source migrations");
    insert_source(
        &source_conn,
        "derived-linux",
        "ceph_rbd",
        "missing-derived.rbd",
    );
    let case_conn = rusqlite::Connection::open_in_memory().expect("open case database");
    let case_root = TempDir::new().expect("create case root");
    let case_id = CaseId("case-1".to_string());
    let source_id = DataSourceId("derived-linux".to_string());
    let mut context = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_id,
    );

    let error = context
        .read_file_header_with_metadata(
            SourceReadFileHint::new(
                FileEntryId("encrypted-rbd-file".to_string()),
                source_id.clone(),
                Some(2),
                "Windows/System32/config/SYSTEM".to_string(),
                4096,
                true,
            ),
            4096,
        )
        .expect_err("EFS metadata hint must fail before opening the RBD provider");

    assert!(matches!(error, FileServiceError::Unsupported(_)));
    assert!(error.to_string().contains("EFS-encrypted"));
    assert!(!error.to_string().contains("Windows/System32/config/SYSTEM"));
    assert!(!error.to_string().contains("missing-derived.rbd"));
    assert!(context.derived_runtime.is_none());
}

#[test]
fn source_read_hint_preserves_encrypted_file_fact() {
    let hint = SourceReadFileHint::new(
        FileEntryId("encrypted-rbd-file".to_string()),
        DataSourceId("derived-linux".to_string()),
        Some(2),
        "Windows/System32/config/SYSTEM".to_string(),
        4096,
        true,
    );

    assert!(hint.encrypted);
}

#[test]
fn partition_candidate_cache_is_bounded() {
    let mut cache = HashMap::new();
    for partition_index in 0..=MAX_SOURCE_PARTITION_CACHE_ENTRIES {
        cache_partition_candidates(&mut cache, partition_index, Vec::new());
    }

    assert_eq!(cache.len(), 1);
    assert!(cache.contains_key(&MAX_SOURCE_PARTITION_CACHE_ENTRIES));
}

#[test]
fn encrypted_bitlocker_partitions_remain_preview_candidates() {
    assert!(
        crate::file_service::viewer::partition::is_previewable_partition_status(
            "encrypted_bitlocker"
        )
    );
    assert!(
        !crate::file_service::viewer::partition::is_previewable_partition_status("unsupported")
    );
}

#[test]
fn bitlocker_plaintext_probe_resolves_ntfs_and_exfat() {
    let mut ntfs = vec![0u8; 0x11000];
    ntfs[3..11].copy_from_slice(b"NTFS    ");
    let mut ntfs = MemoryEvidence::new(ntfs);
    assert_eq!(
        bitlocker::detect_plaintext_filesystem(&mut ntfs).unwrap(),
        "NTFS"
    );

    let mut exfat = vec![0u8; 0x11000];
    exfat[3..11].copy_from_slice(b"EXFAT   ");
    exfat[11..13].copy_from_slice(&512u16.to_le_bytes());
    exfat[108] = 9;
    exfat[510] = 0x55;
    exfat[511] = 0xAA;
    let mut exfat = MemoryEvidence::new(exfat);
    assert_eq!(
        bitlocker::detect_plaintext_filesystem(&mut exfat).unwrap(),
        "EXFAT"
    );
}

#[test]
fn bitlocker_candidate_detection_is_scoped_to_the_bound_source() {
    let source_conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&source_conn).expect("run source migrations");
    source_conn
        .execute_batch(
            "INSERT INTO data_source_partitions
             (id, data_source_id, partition_index, name, kind_label, status,
              offset, length, filesystem)
             VALUES
             ('locked', 'source-a', 2, 'Encrypted', 'BitLocker', 'ready', 4096, 8192, 'NTFS'),
             ('plain', 'source-b', 2, 'Plain', 'NTFS', 'ready', 4096, 8192, 'NTFS');",
        )
        .expect("insert partition metadata");
    let case_conn = rusqlite::Connection::open_in_memory().expect("open case database");
    let case_root = TempDir::new().expect("create case root");
    let case_id = CaseId("case-1".to_string());
    let source_a = DataSourceId("source-a".to_string());
    let source_b = DataSourceId("source-b".to_string());
    let candidate = crate::file_service::viewer::PreviewPartitionCandidate {
        partition_index: 2,
        filesystem_kind: "NTFS".to_string(),
        offset: 4096,
        lvm_identity: None,
    };

    let context_a = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_a,
    );
    let context_b = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_b,
    );

    assert!(context_a.is_bitlocker_candidate(&candidate).unwrap());
    assert!(!context_b.is_bitlocker_candidate(&candidate).unwrap());
}

#[test]
fn locked_bitlocker_candidate_fails_before_plaintext_filesystem_open() {
    let source_conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&source_conn).expect("run source migrations");
    source_conn
        .execute(
            "INSERT INTO data_source_partitions
             (id, data_source_id, partition_index, name, kind_label, status,
              offset, length, filesystem)
             VALUES ('locked', 'source-a', 0, 'Encrypted', 'BitLocker', 'ready',
                     0, 4096, 'NTFS')",
            [],
        )
        .expect("insert partition metadata");
    let raw = TempDir::new().expect("create raw evidence root");
    let raw_path = raw.path().join("locked.raw");
    std::fs::write(&raw_path, vec![0u8; 4096]).expect("write bounded raw image");
    let case_conn = rusqlite::Connection::open_in_memory().expect("open case database");
    let case_root = TempDir::new().expect("create case root");
    let case_id = CaseId("case-1".to_string());
    let source_id = DataSourceId("source-a".to_string());
    let mut context = SourceReadContext::new(
        &source_conn,
        &case_conn,
        case_root.path(),
        &case_id,
        &source_id,
    );
    let descriptor = PreviewDescriptor {
        case_id: case_id.0.clone(),
        file_id: "locked-file".to_string(),
        source_kind: "raw".to_string(),
        source_path: raw_path.display().to_string(),
        partition_index: Some(0),
        filesystem_kind: Some("NTFS".to_string()),
        path: "[P0]/locked.txt".to_string(),
        mime: None,
        size: 1,
        data_source_id: source_id.0.clone(),
        partition_candidates: Vec::new(),
        entry_size: 1,
        entry_modified_at: None,
        ceph_fs: None,
    };
    let candidate = crate::file_service::viewer::PreviewPartitionCandidate {
        partition_index: 0,
        filesystem_kind: "NTFS".to_string(),
        offset: 0,
        lvm_identity: None,
    };
    let mut descriptor = descriptor;
    descriptor.partition_candidates = vec![candidate.clone()];

    let error = match context.open_candidate_block_reader(&descriptor, &candidate) {
        Err(error) => error,
        Ok(_) => panic!("locked volume must not expose a plaintext reader"),
    };

    assert!(matches!(error, FileServiceError::Unsupported(_)));
    assert!(error.to_string().contains("BitLocker volume is locked"));
    assert!(!error
        .to_string()
        .contains(raw_path.to_string_lossy().as_ref()));

    let routed_error =
        match crate::file_service::viewer::open_range_content_for_descriptor_with_context(
            &mut context,
            &descriptor,
        ) {
            Err(error) => error,
            Ok(_) => panic!("locked range path must not bypass BitLocker routing"),
        };
    assert!(matches!(routed_error, FileServiceError::Unsupported(_)));
}
