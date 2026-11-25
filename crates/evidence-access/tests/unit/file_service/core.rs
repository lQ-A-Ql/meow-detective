use super::*;
crate::native_source_test_support!();
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{
    filesystem::root_node, FileSystemDiagnostic, FileSystemDiagnosticKind, FsNode,
};
use persistence_source::repositories::file_repo::FileRepo;
use rusqlite::{params, Connection};
use std::{
    cmp::Ordering as CmpOrdering,
    io::{self, Cursor, Read},
    sync::atomic::{AtomicBool, Ordering},
};
use test_support::NativeSourceFixture;

use crate::file_service::{
    metadata::sorting::{natural_cmp, sort_entries},
    partition_roots::{
        looks_like_raw_fs_root_name, mft_entry_partition_index, partition_placeholder_status,
    },
};
use evidence_core::FileSystemReader;
use transport::commands::{FileSortDirectionDto, FileSortKeyDto, GetFileRowsRequest};

fn standalone_file_entries_connection(source_id: &str) -> (tempfile::TempDir, Connection) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.db");
    let conn = persistence_sqlite::open_or_create_source(&path).unwrap();
    persistence_source::detail::configure_source_identity(&conn, source_id).unwrap();
    persistence_source::detail::configure_source_fjall(&conn, &path).unwrap();
    persistence_source::detail::open_store_for_write(&conn)
        .unwrap()
        .unwrap();
    (root, conn)
}

fn normalized_bare_root_name(
    entry: &FileEntry,
    partitions: &[crate::partition_query_service::PartitionRecord],
) -> String {
    crate::file_service::partition_roots::normalized_bare_root_name_from_partitions(
        entry, partitions,
    )
}

fn partition(index: u32, kind: &str) -> crate::partition_query_service::PartitionRecord {
    crate::partition_query_service::PartitionRecord {
        id: format!("partition:{index}"),
        data_source_id: "fixture-source".to_owned(),
        partition_index: index,
        name: format!("Partition {index}"),
        kind_label: kind.to_owned(),
        status: "ready".to_owned(),
        type_guid: None,
        offset: 0,
        length: 1024,
        filesystem: Some(kind.to_owned()),
        unlock_hint: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        lvm_pv_offsets_json: None,
        lvm_pv_sources_json: None,
        gpt_disk_guid: None,
        gpt_partition_uuid: None,
        filesystem_uuid: None,
    }
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
                    read_only: false,
                    encrypted: false,
                    archive: false,
                    unix_mode: None,
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
                    read_only: false,
                    encrypted: false,
                    archive: false,
                    unix_mode: None,
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
            read_only: false,
            encrypted: false,
            archive: false,
            unix_mode: None,
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
    let (_root, conn) = standalone_file_entries_connection("ds-cancel-enum");
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
    assert_eq!(FileRepo::new(&conn).count_all().unwrap(), 0);
}

#[test]
fn enumerate_filesystem_persists_root_and_child_changed_at() {
    let (_root, conn) = standalone_file_entries_connection("ds-timestamps");
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
    let (_root, conn) = standalone_file_entries_connection("ds-diagnostic");
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
        read_only: false,
        archive: false,
        unix_mode: None,
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
fn indexed_pages_match_service_sorting_for_all_keys_and_visibility_modes() {
    let (_root, connection) = standalone_file_entries_connection("ds");
    let names = [
        "file2.x1",
        "file02.x01",
        "file10.x10",
        "A1",
        "a0",
        "a00",
        "中2",
        "中10",
        "É1",
        "é0",
        "file1.x2",
        "plain",
    ];
    let entries = (0..8)
        .flat_map(|bucket| {
            names.iter().enumerate().map(move |(index, name)| {
                let mut entry = sort_entry(
                    &format!("{name}-{bucket}"),
                    if bucket < 4 {
                        EntryType::Directory
                    } else {
                        EntryType::File
                    },
                    bucket % 2 != 0,
                    false,
                    bucket % 4 >= 2,
                    Some((index % 3) as u64),
                );
                entry.modified_at = chrono::DateTime::from_timestamp(index as i64 % 3 - 2, 123);
                entry
            })
        })
        .collect::<Vec<_>>();
    FileRepo::new(&connection).insert_batch(&entries).unwrap();
    for key in [
        FileSortKeyDto::Name,
        FileSortKeyDto::Size,
        FileSortKeyDto::ModifiedAt,
        FileSortKeyDto::Ext,
    ] {
        for direction in [FileSortDirectionDto::Asc, FileSortDirectionDto::Desc] {
            for show_hidden in [false, true] {
                let mut expected = entries
                    .iter()
                    .filter(|entry| show_hidden || (!entry.hidden && !entry.system))
                    .cloned()
                    .collect::<Vec<_>>();
                sort_entries(&mut expected, key, direction);
                let request = transport::commands::GetFileRowsRequest {
                    offset: 3,
                    limit: 7,
                    show_hidden,
                    sort_key: key,
                    sort_direction: direction,
                    ..Default::default()
                };
                let page = get_file_rows_for_request(&connection, &request).unwrap();
                assert_eq!(page.total_count, expected.len() as u64);
                assert_eq!(
                    page.rows
                        .iter()
                        .map(|row| row.id.as_str())
                        .collect::<Vec<_>>(),
                    expected[3..10]
                        .iter()
                        .map(|entry| entry.id.0.as_str())
                        .collect::<Vec<_>>(),
                    "{key:?} {direction:?} {show_hidden}"
                );
            }
        }
    }
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
    let ds_id = DataSourceId("ds-sort-page".to_string());
    let fixture = NativeSourceFixture::new(&ds_id.0, "c1");

    // Root parent dir + children: 1 dir + files file1,file2,file10.
    let parent = FileEntryId("parent".to_string());
    let repo = FileRepo::new(&fixture.connection);
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
    fixture.seal();

    let request = GetFileRowsRequest {
        parent_id: Some(parent.0.clone()),
        offset: 0,
        limit: 2,
        show_hidden: false,
        sort_key: FileSortKeyDto::Name,
        sort_direction: FileSortDirectionDto::Asc,
    };
    let page = get_file_rows_for_request(&fixture.connection, &request).unwrap();
    assert_eq!(page.total_count, 4);
    assert!(page.truncated);
    // Page 1: directory first, then natural-sorted file1.
    let names: Vec<_> = page.rows.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, vec!["sub", "file1.txt"]);

    let request2 = GetFileRowsRequest {
        offset: 2,
        ..request
    };
    let page2 = get_file_rows_for_request(&fixture.connection, &request2).unwrap();
    let names2: Vec<_> = page2.rows.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names2, vec!["file2.txt", "file10.txt"]);
}

#[test]
fn placeholder_path_encodes_partition_index() {
    let ds_id = DataSourceId("ds-ph-index".to_string());
    let fixture = NativeSourceFixture::new(&ds_id.0, "case-ph-index");

    let id = insert_partition_placeholder_root(
        &fixture.connection,
        &ds_id,
        3,
        "Partition 3 (NTFS)",
        "queued",
    )
    .unwrap();
    let entry = FileRepo::new(&fixture.connection)
        .find_by_id(&id)
        .unwrap()
        .unwrap();
    assert_eq!(entry.path, "__partition_placeholder__/3/queued");
    assert_eq!(partition_placeholder_status(&entry), Some("queued"));
    assert_eq!(
        FileRepo::new(&fixture.connection)
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
        "UPDATE data_sources SET kind='e01', source_path='C:/e' WHERE id=?1",
        params![ds_id],
    )
    .unwrap();
    let database_path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .unwrap();
    let case_root = std::path::Path::new(&database_path)
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .expect("case database parent");
    let storage = crate::source_db::open_fjall_source_read_write(case_root, ds_id).unwrap();
    let snapshot = storage.snapshot();
    let mut records = storage
        .partition_list_by_source(
            &snapshot,
            ds_id,
            None,
            persistence_source::fjall::ScanDirection::Forward,
            256,
        )
        .unwrap()
        .records;
    records.push(persistence_source::fjall::PartitionRecord {
        id: format!("p-{index}"),
        data_source_id: ds_id.to_owned(),
        partition_index: index,
        name: name.to_owned(),
        kind_label: kind.to_owned(),
        status: "supported".to_owned(),
        type_guid: None,
        offset: 0,
        length: 1024,
        filesystem: Some(kind.to_owned()),
        unlock_hint: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        lvm_pv_offsets_json: None,
        lvm_pv_sources_json: None,
        gpt_disk_guid: None,
        gpt_partition_uuid: None,
        filesystem_uuid: None,
    });
    storage
        .partition_replace_by_source(
            ds_id,
            &records,
            persistence_source::fjall::PersistMode::Buffer,
        )
        .unwrap();
    storage.partition_projection_publish(ds_id).unwrap();
    assert_eq!(
        crate::partition_query_service::PartitionQueryRepo::new(conn)
            .count_by_data_source(ds_id)
            .unwrap(),
        records.len() as u64
    );
}

#[test]
fn bare_root_renamed_via_mft_partition_index() {
    let ds_id = "ds-bare-mft";
    let fixture = NativeSourceFixture::new(ds_id, "c1");
    seed_ds_with_partition(&fixture.connection, ds_id, 3, "NTFS");

    let mut entry = sort_entry("\\", EntryType::Directory, false, false, false, None);
    entry.id = FileEntryId("mft:3:5".to_string());
    entry.data_source_id = DataSourceId(ds_id.to_string());

    assert_eq!(
        normalized_bare_root_name(&entry, &[partition(3, "NTFS")]),
        "Partition 3 (NTFS)"
    );
}

#[test]
fn bare_root_renamed_via_sole_partition_when_no_mft_index() {
    let ds_id = "ds-bare-sole";
    let fixture = NativeSourceFixture::new(ds_id, "c1");
    seed_ds_with_partition(&fixture.connection, ds_id, 0, "FAT");

    let mut entry = sort_entry("/", EntryType::Directory, false, false, false, None);
    entry.id = FileEntryId("uuid-root".to_string());
    entry.data_source_id = DataSourceId(ds_id.to_string());

    assert_eq!(
        normalized_bare_root_name(&entry, &[partition(0, "FAT")]),
        "Partition 0 (FAT)"
    );
}

#[test]
fn bare_root_unknown_when_unattributable() {
    let ds_id = "ds-bare-unknown";
    let fixture = NativeSourceFixture::new(ds_id, "c1");
    // Two partitions, non-MFT id → cannot attribute deterministically.
    seed_ds_with_partition(&fixture.connection, ds_id, 0, "NTFS");
    seed_ds_with_partition(&fixture.connection, ds_id, 1, "FAT");

    let mut entry = sort_entry("\\", EntryType::Directory, false, false, false, None);
    entry.id = FileEntryId("uuid-ambiguous".to_string());
    entry.data_source_id = DataSourceId(ds_id.to_string());

    assert_eq!(
        normalized_bare_root_name(&entry, &[partition(0, "NTFS"), partition(1, "FAT")]),
        "Partition ? (UNKNOWN)"
    );
}

#[test]
fn tree_builder_normalizes_residual_bare_root() {
    let ds_id = "ds-tree-bare";
    let fixture = NativeSourceFixture::new(ds_id, "c1");
    seed_ds_with_partition(&fixture.connection, ds_id, 2, "NTFS");

    let mut root = sort_entry("\\", EntryType::Directory, false, false, false, None);
    root.id = FileEntryId("mft:2:5".into());
    root.data_source_id = DataSourceId(ds_id.into());
    FileRepo::new(&fixture.connection)
        .insert_batch(&[root])
        .unwrap();
    fixture.seal();

    let tree = get_file_tree_real_with_visibility(&fixture.connection, false).unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Partition 2 (NTFS)");
    assert_eq!(tree[0].node_type.as_deref(), Some("partition"));
    assert!(!tree.iter().any(|n| n.name == "\\"));
}

#[test]
fn tree_builder_marks_named_lvm_root_as_partition() {
    let ds_id = "ds-tree-lvm";
    let fixture = NativeSourceFixture::new(ds_id, "c1");
    seed_ds_with_partition_name(&fixture.connection, ds_id, 2, "cl/root", "XFS");

    let mut root = sort_entry("cl/root", EntryType::Directory, false, false, false, None);
    root.id = FileEntryId("root-lv".into());
    root.data_source_id = DataSourceId(ds_id.into());
    FileRepo::new(&fixture.connection)
        .insert_batch(&[root])
        .unwrap();
    fixture.seal();

    let tree = get_file_tree_real_with_visibility(&fixture.connection, false).unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "cl/root");
    assert_eq!(tree[0].node_type.as_deref(), Some("partition"));
    assert_eq!(tree[0].status.as_deref(), Some("ready"));
}
