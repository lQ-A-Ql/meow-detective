use super::*;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{
    filesystem::root_node, EvidenceReader, FileSystemDiagnostic, FileSystemDiagnosticKind, FsNode,
};
use fs_ntfs::mft_scanner::MftRecord;
use persistence_sqlite::{open_or_create, repositories::file_repo::FileRepo, runner};
use rusqlite::{params, Connection};
use std::{
    cmp::Ordering as CmpOrdering,
    collections::{HashMap, HashSet},
    io::{self, Cursor, Read, Seek, SeekFrom},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::TempDir;

use crate::file_service::{
    metadata::sorting::{natural_cmp, sort_entries},
    partition_roots::{
        looks_like_raw_fs_root_name, mft_entry_partition_index, partition_placeholder_status,
    },
};
use evidence_core::FileSystemReader;
use transport::commands::{FileSortDirectionDto, FileSortKeyDto, GetFileRowsRequest};

fn normalized_bare_root_name(conn: &rusqlite::Connection, entry: &FileEntry) -> String {
    let partitions = persistence_sqlite::repositories::partition_repo::PartitionRepo::new(conn)
        .find_by_data_source(&entry.data_source_id.0)
        .unwrap_or_default();
    crate::file_service::partition_roots::normalized_bare_root_name_from_partitions(
        entry,
        &partitions,
    )
}

struct CancelAfterRootFs;

impl FileSystemReader for CancelAfterRootFs {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if path.is_empty() {
            Ok(vec![
                FsNode {
                    name: "first.txt".to_string(),
                    path: "first.txt".to_string(),
                    is_dir: false,
                    size: 1,
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                },
                FsNode {
                    name: "second.txt".to_string(),
                    path: "second.txt".to_string(),
                    is_dir: false,
                    size: 1,
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                },
            ])
        } else {
            Ok(Vec::new())
        }
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        "cancel-after-root"
    }
}

struct TimestampedFs {
    root_changed_at: chrono::DateTime<chrono::Utc>,
    child_changed_at: chrono::DateTime<chrono::Utc>,
}

struct PartialDirectoryDiagnosticFs {
    diagnostics: std::cell::RefCell<Vec<FileSystemDiagnostic>>,
}

impl PartialDirectoryDiagnosticFs {
    fn new() -> Self {
        Self {
            diagnostics: std::cell::RefCell::new(vec![FileSystemDiagnostic::new(
                FileSystemDiagnosticKind::DirectoryPartial,
                "typed partial directory",
            )]),
        }
    }
}

impl FileSystemReader for PartialDirectoryDiagnosticFs {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, _path: &str) -> io::Result<Vec<FsNode>> {
        Ok(Vec::new())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn take_diagnostics(&self) -> Vec<FileSystemDiagnostic> {
        std::mem::take(&mut *self.diagnostics.borrow_mut())
    }

    fn data_source_name(&self) -> &str {
        "partial-directory-diagnostic"
    }
}

impl FileSystemReader for TimestampedFs {
    fn root(&self) -> io::Result<FsNode> {
        let mut root = root_node();
        root.changed_at = Some(self.root_changed_at);
        Ok(root)
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if !path.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![FsNode {
            name: "timestamped.txt".to_string(),
            path: "timestamped.txt".to_string(),
            is_dir: false,
            size: 1,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: Some(self.child_changed_at),
        }])
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        "timestamped"
    }
}

#[test]
fn enumerate_filesystem_cancel_rolls_back_transaction() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0003_file_entries.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
            "../../../../persistence-sqlite/src/migrations/scripts/0022_file_entry_visibility_flags.sql"
        ))
        .unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0042_file_entry_encrypted.sql"
    ))
    .unwrap();
    let cancel = AtomicBool::new(false);
    let ds_id = DataSourceId("ds-cancel-enum".to_string());
    let fs = CancelAfterRootFs;

    let Err(err) = enumerate_filesystem_with_root_name_and_cancel(
        &conn,
        &ds_id,
        &fs,
        None,
        Some(&|_| cancel.store(true, Ordering::Relaxed)),
        Some(&cancel),
    ) else {
        panic!("expected cancellation to roll back enumeration transaction");
    };

    assert!(err.to_string().contains("Enumeration cancelled"));
    let row_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(row_count, 0);
}

#[test]
fn enumerate_filesystem_persists_root_and_child_changed_at() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0003_file_entries.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0022_file_entry_visibility_flags.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0042_file_entry_encrypted.sql"
    ))
    .unwrap();
    let root_changed_at = chrono::DateTime::from_timestamp(1_700_000_000, 123).unwrap();
    let child_changed_at = chrono::DateTime::from_timestamp(1_800_000_000, 456).unwrap();
    let fs = TimestampedFs {
        root_changed_at,
        child_changed_at,
    };
    let data_source_id = DataSourceId("ds-timestamps".to_string());

    enumerate_filesystem(&conn, &data_source_id, &fs).unwrap();

    let repo = FileRepo::new(&conn);
    let roots = repo.find_root_entries().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].changed_at, Some(root_changed_at));
    let children = repo.find_children(&roots[0].id).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].changed_at, Some(child_changed_at));
}

#[test]
fn enumerate_filesystem_preserves_typed_completeness_diagnostics() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0003_file_entries.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0022_file_entry_visibility_flags.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0042_file_entry_encrypted.sql"
    ))
    .unwrap();
    let stats = enumerate_filesystem(
        &conn,
        &DataSourceId("ds-diagnostic".to_string()),
        &PartialDirectoryDiagnosticFs::new(),
    )
    .unwrap();

    assert_eq!(stats.incomplete_catalog_diagnostic_count(), 1);
    assert_eq!(stats.diagnostics[0].path.as_deref(), Some(""));
    assert_eq!(stats.warnings, ["typed partial directory"]);
}

#[test]
fn safe_path_rejects_dot_dot_traversal() {
    assert!(safe_relative_path("../etc/passwd").is_err());
    assert!(safe_relative_path("foo/../../bar").is_err());
    assert!(safe_relative_path("..\\windows\\system32").is_err());
}

#[test]
fn safe_path_rejects_url_encoded_traversal() {
    assert!(safe_relative_path("%2e%2e%2fetc%2fpasswd").is_err());
    assert!(safe_relative_path("foo%2f%2e%2e%2fbar").is_err());
}

#[test]
fn safe_path_rejects_null_byte() {
    assert!(safe_relative_path("file.txt\0.jpg").is_err());
}

#[test]
fn safe_path_rejects_absolute_path() {
    assert!(safe_relative_path("/etc/passwd").is_err());
}

#[test]
fn safe_path_accepts_valid_paths() {
    assert!(safe_relative_path("documents/file.txt").is_ok());
    assert!(safe_relative_path("a/b/c.txt").is_ok());
    assert!(safe_relative_path("simple.txt").is_ok());
}

#[test]
fn safe_path_rejects_empty_path() {
    assert!(safe_relative_path("").is_err());
}

#[test]
fn safe_path_rejects_windows_reserved_names() {
    assert!(safe_relative_path("CON").is_err());
    assert!(safe_relative_path("NUL.txt").is_err());
    assert!(safe_relative_path("COM1").is_err());
    assert!(safe_relative_path("LPT1.dat").is_err());
}

#[test]
fn mft_root_record_becomes_tree_root() {
    let records = vec![
        MftRecord {
            record_number: 5,
            sequence_number: 0,
            name: ".".to_string(),
            parent_ref: 5,
            is_dir: true,
            size: 0,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hidden: false,
            system: false,
            encrypted: false,
            deleted: false,
            is_valid: true,
        },
        MftRecord {
            record_number: 42,
            sequence_number: 0,
            name: "Windows".to_string(),
            parent_ref: 5,
            is_dir: true,
            size: 0,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hidden: false,
            system: false,
            encrypted: true,
            deleted: false,
            is_valid: true,
        },
    ];

    let entries = records_to_file_entries(&records, &DataSourceId("ds".to_string()));

    let root = entries
        .iter()
        .find(|entry| entry.id.0 == "mft:5")
        .expect("root MFT record should be retained");
    assert_eq!(root.name, "/");
    assert!(root.parent_id.is_none());

    let child = entries
        .iter()
        .find(|entry| entry.id.0 == "mft:42")
        .expect("child MFT record should be retained");
    assert_eq!(
        child.parent_id.as_ref().map(|id| id.0.as_str()),
        Some("mft:5")
    );
    assert!(child.encrypted);
}

#[test]
fn mft_orphan_records_are_anchored_to_root() {
    let mut path_map = HashMap::new();
    path_map.insert("5".to_string(), (None, "\\".to_string(), true));
    path_map.insert(
        "42".to_string(),
        (Some("999".to_string()), "Orphan".to_string(), true),
    );

    assert_eq!(
        mft_parent_entry_id("42", Some("999"), &path_map),
        Some("mft:5".to_string())
    );
    assert_eq!(mft_parent_entry_id("5", None, &path_map), None);
}

#[test]
fn mft_deleted_orphan_path_uses_deleted_orphans_prefix() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0003_file_entries.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
            "../../../../persistence-sqlite/src/migrations/scripts/0022_file_entry_visibility_flags.sql"
        ))
        .unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/0042_file_entry_encrypted.sql"
    ))
    .unwrap();
    let ds_id = DataSourceId("ds-deleted-orphan".to_string());
    let mut entries = records_to_file_entries(
        &[
            MftRecord {
                record_number: 5,
                sequence_number: 0,
                name: ".".to_string(),
                parent_ref: 5,
                is_dir: true,
                size: 0,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hidden: false,
                system: false,
                encrypted: false,
                deleted: false,
                is_valid: true,
            },
            MftRecord {
                record_number: 77,
                sequence_number: 0,
                name: "old.txt".to_string(),
                parent_ref: 999,
                is_dir: false,
                size: 12,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hidden: false,
                system: false,
                encrypted: false,
                deleted: true,
                is_valid: true,
            },
        ],
        &ds_id,
    );
    for entry in &mut entries {
        entry.parent_id = None;
    }
    FileRepo::new(&conn).insert_batch(&entries).unwrap();
    let mut path_map = HashMap::new();
    let mut deleted_records = HashSet::new();
    for entry in &entries {
        add_entry_to_path_map(&mut path_map, &mut deleted_records, entry);
    }

    update_entry_paths(&conn, &ds_id, &path_map, &deleted_records, 0).unwrap();
    update_entry_parent_ids(&conn, &ds_id, &path_map).unwrap();

    let (path, parent_id, deleted): (String, Option<String>, i32) = conn
        .query_row(
            "SELECT path, parent_id, deleted FROM file_entries WHERE id = 'mft:77'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(path, "[P0]/$DeletedOrphans/77-old.txt");
    assert_eq!(parent_id.as_deref(), Some("mft:5"));
    assert_eq!(deleted, 1);
}

struct SliceEvidenceReader {
    data: Vec<u8>,
    pos: u64,
    info: evidence_core::ReaderInfo,
}

impl SliceEvidenceReader {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            pos: 0,
            info: evidence_core::ReaderInfo {
                path: PathBuf::from("slice"),
                size: 0,
                kind: "test".to_string(),
            },
        }
    }
}

impl Read for SliceEvidenceReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = self.pos as usize;
        if start >= self.data.len() {
            return Ok(0);
        }
        let count = buf.len().min(self.data.len() - start);
        buf[..count].copy_from_slice(&self.data[start..start + count]);
        self.pos += count as u64;
        Ok(count)
    }
}

impl Seek for SliceEvidenceReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(value) => value as i128,
            SeekFrom::End(value) => self.data.len() as i128 + value as i128,
            SeekFrom::Current(value) => self.pos as i128 + value as i128,
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "negative seek",
            ));
        }
        self.pos = next as u64;
        Ok(self.pos)
    }
}

impl EvidenceReader for SliceEvidenceReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

#[test]
fn read_ntfs_mft_stream_stitches_fragmented_runs() {
    let mut disk = vec![0u8; 4096];
    disk[1024..1536].fill(b'A');
    disk[3072..3584].fill(b'B');
    let mut reader = SliceEvidenceReader::new(disk);
    let mut out = vec![0u8; 1024];

    read_ntfs_mft_stream(&mut reader, 0, 512, &[(2, 1), (6, 1)], 0, &mut out).unwrap();

    assert!(out[..512].iter().all(|byte| *byte == b'A'));
    assert!(out[512..].iter().all(|byte| *byte == b'B'));
}

#[test]
fn read_ntfs_mft_stream_stitches_read_crossing_run_boundary() {
    let mut disk = vec![0u8; 4096];
    disk[1024..1536].fill(b'A');
    disk[3072..3584].fill(b'B');
    let mut reader = SliceEvidenceReader::new(disk);
    let mut out = vec![0u8; 512];

    read_ntfs_mft_stream(&mut reader, 0, 512, &[(2, 1), (6, 1)], 256, &mut out).unwrap();

    assert!(out[..256].iter().all(|byte| *byte == b'A'));
    assert!(out[256..].iter().all(|byte| *byte == b'B'));
}

#[test]
fn read_ntfs_mft_stream_rejects_negative_lcn() {
    let mut reader = SliceEvidenceReader::new(vec![0u8; 1024]);
    let mut out = vec![0u8; 512];

    let err = read_ntfs_mft_stream(&mut reader, 0, 512, &[(-1, 1)], 0, &mut out)
        .expect_err("negative LCN must fail closed");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn parse_ntfs_data_runs_decodes_fragmented_runs() {
    let runs = parse_ntfs_data_runs(&[0x11, 0x02, 0x05, 0x11, 0x03, 0x07, 0x00]).unwrap();

    assert_eq!(runs, vec![(5, 2), (12, 3)]);
}

#[test]
fn safe_path_allows_normal_names() {
    assert!(safe_relative_path("config.txt").is_ok());
    assert!(safe_relative_path("data.json").is_ok());
    assert!(safe_relative_path("folder/subfolder/file.log").is_ok());
}

// ------------------------------------------------------------------
// Service-layer sort comparator
// ------------------------------------------------------------------

fn sort_entry(
    name: &str,
    entry_type: EntryType,
    hidden: bool,
    system: bool,
    deleted: bool,
    size: Option<u64>,
) -> FileEntry {
    FileEntry {
        id: FileEntryId(format!("id-{name}")),
        parent_id: None,
        data_source_id: DataSourceId("ds".to_string()),
        path: name.to_string(),
        name: name.to_string(),
        entry_type,
        size,
        ext: name
            .rsplit('.')
            .next()
            .filter(|e| *e != name)
            .map(|e| e.to_string()),
        deleted,
        hidden,
        system,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn names_after_sort(
    mut entries: Vec<FileEntry>,
    key: FileSortKeyDto,
    dir: FileSortDirectionDto,
) -> Vec<String> {
    sort_entries(&mut entries, key, dir);
    entries.into_iter().map(|e| e.name).collect()
}

#[test]
fn natural_sort_orders_numeric_suffixes_like_explorer() {
    assert_eq!(natural_cmp("file2", "file10"), CmpOrdering::Less);
    assert_eq!(natural_cmp("file10", "file2"), CmpOrdering::Greater);
    assert_eq!(natural_cmp("img9", "img09"), CmpOrdering::Less); // equal magnitude, shorter raw run first
    assert_eq!(natural_cmp("Alpha", "alpha"), CmpOrdering::Less); // case-insensitive then raw
}

#[test]
fn sort_keeps_directories_before_files_even_when_descending() {
    let entries = vec![
        sort_entry("zeta.txt", EntryType::File, false, false, false, Some(1)),
        sort_entry("alpha", EntryType::Directory, false, false, false, None),
        sort_entry("beta", EntryType::Directory, false, false, false, None),
    ];
    let ordered = names_after_sort(entries, FileSortKeyDto::Name, FileSortDirectionDto::Desc);
    // Directories first (fixed), files after — even under descending name sort.
    assert_eq!(ordered, vec!["beta", "alpha", "zeta.txt"]);
}

#[test]
fn sort_uses_natural_name_order_for_files() {
    let entries = vec![
        sort_entry("file10.log", EntryType::File, false, false, false, Some(1)),
        sort_entry("file2.log", EntryType::File, false, false, false, Some(1)),
        sort_entry("file1.log", EntryType::File, false, false, false, Some(1)),
    ];
    let ordered = names_after_sort(entries, FileSortKeyDto::Name, FileSortDirectionDto::Asc);
    assert_eq!(ordered, vec!["file1.log", "file2.log", "file10.log"]);
}

#[test]
fn sort_sinks_hidden_system_deleted_after_normal() {
    let entries = vec![
        sort_entry("normal.txt", EntryType::File, false, false, false, Some(1)),
        sort_entry("deleted.txt", EntryType::File, false, false, true, Some(1)),
        sort_entry("hidden.txt", EntryType::File, true, false, false, Some(1)),
        sort_entry("both.txt", EntryType::File, true, false, true, Some(1)),
    ];
    let ordered = names_after_sort(entries, FileSortKeyDto::Name, FileSortDirectionDto::Asc);
    // Buckets: normal(0) < hidden/system(1) < deleted(2) < hidden+deleted(3).
    assert_eq!(
        ordered,
        vec!["normal.txt", "hidden.txt", "deleted.txt", "both.txt"]
    );
}

#[test]
fn sort_status_buckets_are_fixed_under_descending_name() {
    let entries = vec![
        sort_entry("aaa.txt", EntryType::File, true, false, false, Some(1)), // hidden
        sort_entry("zzz.txt", EntryType::File, false, false, false, Some(1)), // normal
    ];
    let ordered = names_after_sort(entries, FileSortKeyDto::Name, FileSortDirectionDto::Desc);
    // Normal bucket still precedes hidden bucket regardless of direction.
    assert_eq!(ordered, vec!["zzz.txt", "aaa.txt"]);
}

#[test]
fn sort_by_size_descending_within_files() {
    let entries = vec![
        sort_entry("small.bin", EntryType::File, false, false, false, Some(10)),
        sort_entry("big.bin", EntryType::File, false, false, false, Some(9000)),
        sort_entry("mid.bin", EntryType::File, false, false, false, Some(500)),
    ];
    let ordered = names_after_sort(entries, FileSortKeyDto::Size, FileSortDirectionDto::Desc);
    assert_eq!(ordered, vec!["big.bin", "mid.bin", "small.bin"]);
}

#[test]
fn get_file_rows_sorts_full_set_then_paginates() {
    let tmp = TempDir::new().unwrap();
    let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-sort-page".to_string());
    conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES ('c1','C','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES (?1,'c1','ds','logical_directory','C:/e','2026-01-01T00:00:00Z')",
            params![ds_id.0],
        )
        .unwrap();

    // Root parent dir + children: 1 dir + files file1,file2,file10.
    let parent = FileEntryId("parent".to_string());
    let repo = FileRepo::new(&conn);
    let mut parent_entry = sort_entry("root", EntryType::Directory, false, false, false, None);
    parent_entry.id = parent.clone();
    parent_entry.data_source_id = ds_id.clone();
    repo.insert_batch(&[parent_entry]).unwrap();

    let mut children = Vec::new();
    for n in ["sub", "file10.txt", "file2.txt", "file1.txt"] {
        let is_dir = n == "sub";
        let mut child = sort_entry(
            n,
            if is_dir {
                EntryType::Directory
            } else {
                EntryType::File
            },
            false,
            false,
            false,
            if is_dir { None } else { Some(1) },
        );
        child.id = FileEntryId(format!("c-{n}"));
        child.parent_id = Some(parent.clone());
        child.data_source_id = ds_id.clone();
        children.push(child);
    }
    repo.insert_batch(&children).unwrap();

    let request = GetFileRowsRequest {
        parent_id: Some(parent.0.clone()),
        offset: 0,
        limit: 2,
        show_hidden: false,
        sort_key: FileSortKeyDto::Name,
        sort_direction: FileSortDirectionDto::Asc,
    };
    let page = get_file_rows_for_request(&conn, &request).unwrap();
    assert_eq!(page.total_count, 4);
    assert!(page.truncated);
    // Page 1: directory first, then natural-sorted file1.
    let names: Vec<_> = page.rows.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, vec!["sub", "file1.txt"]);

    let request2 = GetFileRowsRequest {
        offset: 2,
        ..request
    };
    let page2 = get_file_rows_for_request(&conn, &request2).unwrap();
    let names2: Vec<_> = page2.rows.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names2, vec!["file2.txt", "file10.txt"]);
}

#[test]
fn placeholder_path_encodes_partition_index() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    let ds_id = DataSourceId("ds-ph-index".to_string());

    let id = insert_partition_placeholder_root(&conn, &ds_id, 3, "Partition 3 (NTFS)", "queued")
        .unwrap();
    let entry = FileRepo::new(&conn).find_by_id(&id).unwrap().unwrap();
    assert_eq!(entry.path, "__partition_placeholder__/3/queued");
    assert_eq!(partition_placeholder_status(&entry), Some("queued"));
    assert_eq!(
        FileRepo::new(&conn)
            .find_partition_index_by_id(&id)
            .unwrap(),
        Some(3)
    );
}

// ------------------------------------------------------------------
// Stage C: read-side defensive root normalization
// ------------------------------------------------------------------

#[test]
fn raw_fs_root_name_detection() {
    assert!(looks_like_raw_fs_root_name("\\"));
    assert!(looks_like_raw_fs_root_name("/"));
    assert!(looks_like_raw_fs_root_name("."));
    assert!(!looks_like_raw_fs_root_name("Windows"));
    assert!(!looks_like_raw_fs_root_name("EFI"));
    assert!(!looks_like_raw_fs_root_name("Partition 0 (NTFS)"));
}

#[test]
fn mft_entry_partition_index_parsing() {
    assert_eq!(mft_entry_partition_index("mft:3:5"), Some(3));
    assert_eq!(mft_entry_partition_index("mft:0:42"), Some(0));
    assert_eq!(mft_entry_partition_index("mft:5"), None); // legacy, no partition
    assert_eq!(mft_entry_partition_index("uuid-abc"), None);
}

fn seed_ds_with_partition(conn: &Connection, ds_id: &str, index: u32, kind: &str) {
    seed_ds_with_partition_name(conn, ds_id, index, &format!("Part {index}"), kind);
}

fn seed_ds_with_partition_name(conn: &Connection, ds_id: &str, index: u32, name: &str, kind: &str) {
    conn.execute(
            "INSERT OR IGNORE INTO cases (id, name, created_at, updated_at) VALUES ('c1','C','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT OR IGNORE INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES (?1,'c1','ds','e01','C:/e','2026-01-01T00:00:00Z')",
            params![ds_id],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO data_source_partitions (id, data_source_id, partition_index, name, kind_label, status, offset, length)
             VALUES (?1, ?2, ?3, ?4, ?5, 'supported', 0, 1024)",
            params![format!("p-{index}"), ds_id, index, name, kind],
        )
        .unwrap();
}

#[test]
fn bare_root_renamed_via_mft_partition_index() {
    let tmp = TempDir::new().unwrap();
    let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = "ds-bare-mft";
    seed_ds_with_partition(&conn, ds_id, 3, "NTFS");

    let mut entry = sort_entry("\\", EntryType::Directory, false, false, false, None);
    entry.id = FileEntryId("mft:3:5".to_string());
    entry.data_source_id = DataSourceId(ds_id.to_string());

    assert_eq!(
        normalized_bare_root_name(&conn, &entry),
        "Partition 3 (NTFS)"
    );
}

#[test]
fn bare_root_renamed_via_sole_partition_when_no_mft_index() {
    let tmp = TempDir::new().unwrap();
    let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = "ds-bare-sole";
    seed_ds_with_partition(&conn, ds_id, 0, "FAT");

    let mut entry = sort_entry("/", EntryType::Directory, false, false, false, None);
    entry.id = FileEntryId("uuid-root".to_string());
    entry.data_source_id = DataSourceId(ds_id.to_string());

    assert_eq!(
        normalized_bare_root_name(&conn, &entry),
        "Partition 0 (FAT)"
    );
}

#[test]
fn bare_root_unknown_when_unattributable() {
    let tmp = TempDir::new().unwrap();
    let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = "ds-bare-unknown";
    // Two partitions, non-MFT id → cannot attribute deterministically.
    seed_ds_with_partition(&conn, ds_id, 0, "NTFS");
    seed_ds_with_partition(&conn, ds_id, 1, "FAT");

    let mut entry = sort_entry("\\", EntryType::Directory, false, false, false, None);
    entry.id = FileEntryId("uuid-ambiguous".to_string());
    entry.data_source_id = DataSourceId(ds_id.to_string());

    assert_eq!(
        normalized_bare_root_name(&conn, &entry),
        "Partition ? (UNKNOWN)"
    );
}

#[test]
fn tree_builder_normalizes_residual_bare_root() {
    let tmp = TempDir::new().unwrap();
    let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = "ds-tree-bare";
    seed_ds_with_partition(&conn, ds_id, 2, "NTFS");

    // A residual bare `\` root directly in the main DB (simulates an older
    // case that escaped staging folding).
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size)
             VALUES ('mft:2:5', NULL, ?1, '', '\\', 'directory', 0)",
        params![ds_id],
    )
    .unwrap();

    let tree = get_file_tree_real_with_visibility(&conn, false).unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Partition 2 (NTFS)");
    assert_eq!(tree[0].node_type.as_deref(), Some("partition"));
    assert!(!tree.iter().any(|n| n.name == "\\"));
}

#[test]
fn tree_builder_marks_named_lvm_root_as_partition() {
    let tmp = TempDir::new().unwrap();
    let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    let ds_id = "ds-tree-lvm";
    seed_ds_with_partition_name(&conn, ds_id, 2, "cl/root", "XFS");

    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size)
             VALUES ('root-lv', NULL, ?1, '', 'cl/root', 'directory', 0)",
        params![ds_id],
    )
    .unwrap();

    let tree = get_file_tree_real_with_visibility(&conn, false).unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "cl/root");
    assert_eq!(tree[0].node_type.as_deref(), Some("partition"));
    assert_eq!(tree[0].status.as_deref(), Some("ready"));
}
