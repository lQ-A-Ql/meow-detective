use super::*;
use super::{
    batch_sink::{prepare_mft_insert, EnumerationStats},
    ntfs::{
        mft_scan::{
            open_partition_evidence_reader, read_ntfs_mft_parameters_at, read_ntfs_mft_stream,
        },
        path_reconstruction::{
            directory_reference_sequence_matches,
            mft_directory_index_backfill_actions_with_representatives,
            update_mft_staging_paths_via_sqlite, MftCatalog, MftDirectoryIndexBackfillAction,
        },
        size_reconciliation::apply_authoritative_size,
        validation::validate_mft_staging_shape,
    },
    partition_work::enumerate_single_partition,
};
use crate::staging;
use domain::DataSourceId;
use evidence_core::filesystem::root_node;
use evidence_core::{EvidenceReader, FileSystemReader, FsNode, PartitionWindowReader};
use fs_ntfs::mft_scanner::{MftRecord, MftScanner};
use fs_ntfs::{NtfsDirectoryEntry, NtfsReader};
use image_e01::E01Reader;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use volume_bitlocker::{unlock_volume_with_recovery_password, Passphrase};

struct FakeFsReader {
    name: String,
    entry_count: usize,
    root_list_delay: Duration,
    active_lists: Arc<AtomicUsize>,
    max_active_lists: Arc<AtomicUsize>,
}

struct ActiveListGuard<'a> {
    active_lists: &'a AtomicUsize,
}

impl Drop for ActiveListGuard<'_> {
    fn drop(&mut self) {
        self.active_lists.fetch_sub(1, Ordering::SeqCst);
    }
}

impl FakeFsReader {
    fn new(
        name: impl Into<String>,
        entry_count: usize,
        root_list_delay: Duration,
        active_lists: Arc<AtomicUsize>,
        max_active_lists: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            name: name.into(),
            entry_count,
            root_list_delay,
            active_lists,
            max_active_lists,
        }
    }

    fn root_files(&self) -> Vec<FsNode> {
        (0..self.entry_count)
            .map(|index| FsNode {
                name: format!("file-{index}.txt"),
                path: format!("/file-{index}.txt"),
                is_dir: false,
                size: 1,
                hidden: false,
                system: false,
                read_only: false,
                encrypted: false,
                archive: false,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
            })
            .collect()
    }
}

impl FileSystemReader for FakeFsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if !path.is_empty() {
            return Ok(Vec::new());
        }

        let active = self.active_lists.fetch_add(1, Ordering::SeqCst) + 1;
        update_max_active(&self.max_active_lists, active);
        let _guard = ActiveListGuard {
            active_lists: &self.active_lists,
        };

        if !self.root_list_delay.is_zero() {
            std::thread::sleep(self.root_list_delay);
        }

        Ok(self.root_files())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        &self.name
    }
}

struct FakeEvidenceReader {
    cursor: Cursor<Vec<u8>>,
    info: evidence_core::ReaderInfo,
}

impl FakeEvidenceReader {
    fn new(data: Vec<u8>) -> Self {
        Self {
            cursor: Cursor::new(data),
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("fake-parallel-enum"),
                size: 0,
                kind: "fake-parallel-enum".to_string(),
            },
        }
    }
}

impl Read for FakeEvidenceReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buf)
    }
}

impl Seek for FakeEvidenceReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(pos)
    }
}

impl EvidenceReader for FakeEvidenceReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

fn update_max_active(max_active_lists: &AtomicUsize, active: usize) {
    let mut current = max_active_lists.load(Ordering::SeqCst);
    while active > current {
        match max_active_lists.compare_exchange(current, active, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn fake_partition_work(
    index: usize,
    entry_count: usize,
    root_list_delay: Duration,
    active_lists: Arc<AtomicUsize>,
    max_active_lists: Arc<AtomicUsize>,
) -> PartitionWork {
    PartitionWork {
        index,
        name: format!("Partition {index}"),
        fs_kind: "FakeFs".to_string(),
        fs: Box::new(FakeFsReader::new(
            format!("fake-{index}"),
            entry_count,
            root_list_delay,
            active_lists,
            max_active_lists,
        )),
        source_path: PathBuf::from(format!("fake-{index}.img")),
        source_kind: "Raw".to_string(),
        volume_offset: 0,
    }
}

fn fake_partitions(
    count: usize,
    entry_count: usize,
    root_list_delay: Duration,
) -> Vec<PartitionWork> {
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    (0..count)
        .map(|index| {
            fake_partition_work(
                index,
                entry_count,
                root_list_delay,
                active_lists.clone(),
                max_active_lists.clone(),
            )
        })
        .collect()
}

fn fake_mft_record(record_number: u64, parent_ref: u64, name: &str, is_dir: bool) -> MftRecord {
    MftRecord {
        record_number,
        sequence_number: 0,
        name: name.to_string(),
        parent_ref,
        is_dir,
        size: if is_dir { 0 } else { 12 },
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hidden: false,
        system: false,
        read_only: false,
        encrypted: false,
        archive: false,
        has_attribute_list: false,
        deleted: false,
        is_valid: true,
    }
}

fn fake_deleted_mft_record(
    record_number: u64,
    parent_ref: u64,
    name: &str,
    is_dir: bool,
) -> MftRecord {
    let mut record = fake_mft_record(record_number, parent_ref, name, is_dir);
    record.deleted = true;
    record
}

fn fake_ntfs_index_entry(
    mft_ref: u64,
    name: impl Into<String>,
    is_dir: bool,
) -> NtfsDirectoryEntry {
    NtfsDirectoryEntry {
        name: name.into(),
        is_dir,
        size: if is_dir { 0 } else { 12 },
        mft_ref,
        hidden: false,
        system: false,
        read_only: false,
        encrypted: false,
        archive: false,
    }
}

fn mft_directory_index_backfill_actions(
    path_map: &mut HashMap<String, (Option<String>, String, bool)>,
    directory_ref: u64,
    entries: Vec<NtfsDirectoryEntry>,
) -> Vec<MftDirectoryIndexBackfillAction> {
    mft_directory_index_backfill_actions_with_representatives(
        path_map,
        &mut HashMap::new(),
        directory_ref,
        entries,
    )
}

#[test]
fn test_default_worker_count() {
    let count = default_worker_count();
    assert!(count >= 1);
}

#[test]
fn test_resolve_worker_count() {
    assert_eq!(resolve_worker_count(None), default_worker_count());
    assert_eq!(resolve_worker_count(Some(0)), default_worker_count());
    assert_eq!(resolve_worker_count(Some(2)), 2.min(default_worker_count()));
    assert_eq!(resolve_worker_count(Some(999)), default_worker_count());
}

#[test]
fn test_resolve_worker_count_one() {
    assert_eq!(resolve_worker_count(Some(1)), 1);
}

#[test]
fn ntfs_mft_fast_path_writes_partition_prefixed_ids() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let mut encrypted_record = fake_mft_record(43, 42, "notepad.exe", false);
    encrypted_record.encrypted = true;
    let records = vec![
        fake_mft_record(5, 5, ".", true),
        fake_mft_record(42, 5, "Windows", true),
        encrypted_record,
    ];
    stage_mft_records_for_test(&conn, &records, "ds-1", 3);

    let root_parent: Option<String> = conn
        .query_row(
            "SELECT parent_id FROM file_entries WHERE id = 'mft:3:5'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let child_parent: Option<String> = conn
        .query_row(
            "SELECT parent_id FROM file_entries WHERE id = 'mft:3:43'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(root_parent.is_none());
    assert_eq!(child_parent.as_deref(), Some("mft:3:42"));
    let partition_indexes: Vec<i64> = conn
        .prepare("SELECT partition_index FROM file_entries ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(partition_indexes, vec![3, 3, 3]);
    let encrypted: bool = conn
        .query_row(
            "SELECT encrypted <> 0 FROM file_entries WHERE id = 'mft:3:43'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(encrypted);
}

#[test]
fn authoritative_external_data_size_updates_staging_and_statistics() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let mut record = fake_mft_record(34_971, 5, "SOFTWARE", false);
    record.size = 8_192;
    record.has_attribute_list = true;
    stage_mft_records_for_test(&conn, &[record], "ds-size", 3);
    let mut stats = EnumerationStats {
        file_count: 1,
        total_size: 8_192,
        ..EnumerationStats::default()
    };

    assert!(
        apply_authoritative_size(&conn, "ds-size", 3, 34_971, 8_192, 75_497_472, &mut stats,)
            .unwrap()
    );
    let size: u64 = conn
        .query_row(
            "SELECT size FROM file_entries WHERE id = 'mft:3:34971'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(size, 75_497_472);
    assert_eq!(stats.total_size, 75_497_472);
}

#[test]
fn recursive_enum_persists_partition_index_for_every_row() {
    let tmp = tempfile::TempDir::new().unwrap();
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    let partition = fake_partition_work(7, 3, Duration::ZERO, active_lists, max_active_lists);

    let result = enumerate_single_partition(
        tmp.path(),
        "ds-recursive-partition-index",
        partition,
        &AtomicBool::new(false),
        None,
    );

    assert!(result.error.is_none());
    let conn =
        staging::open_partition_staging(tmp.path(), "ds-recursive-partition-index", 7).unwrap();
    let missing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE partition_index IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let wrong: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE partition_index <> 7",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(missing, 0);
    assert_eq!(wrong, 0);
}

fn insert_staging_entry(conn: &rusqlite::Connection, id: &str, ds_id: &str) {
    conn.execute(
        "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES (?1, ?2, '', ?3, 'File')",
        rusqlite::params![id, ds_id, id],
    )
    .unwrap();
}

fn stage_mft_records_for_test(
    conn: &rusqlite::Connection,
    records: &[MftRecord],
    data_source_id: &str,
    partition_index: usize,
) -> MftCatalog {
    let mut statement = prepare_mft_insert(conn).unwrap();
    let mut catalog = MftCatalog::default();
    catalog
        .stage_records(&mut statement, records, data_source_id, partition_index)
        .unwrap();
    drop(statement);
    catalog
}

#[test]
fn ntfs_mft_updates_paths_and_parent_ids() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();

    let records = vec![
        fake_mft_record(5, 5, ".", true),
        fake_mft_record(42, 5, "Windows", true),
        fake_mft_record(43, 42, "notepad.exe", false),
    ];
    let catalog = stage_mft_records_for_test(&conn, &records, "ds-mft", 3);
    catalog.update_staging_paths(&conn, "ds-mft", 3).unwrap();

    let (path, parent_id): (String, Option<String>) = conn
        .query_row(
            "SELECT path, parent_id FROM file_entries WHERE id = 'mft:3:43'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(path, "Windows/notepad.exe");
    assert_eq!(parent_id.as_deref(), Some("mft:3:42"));
}

#[test]
fn ntfs_directory_index_backfill_traverses_newly_discovered_directories() {
    let mut path_map = HashMap::new();
    path_map.insert("5".to_string(), (None, "/".to_string(), true));
    let mut indexes = HashMap::new();
    indexes.insert(5, vec![fake_ntfs_index_entry(42, "Users", true)]);
    indexes.insert(42, vec![fake_ntfs_index_entry(43, "Liu Yang", true)]);
    indexes.insert(43, vec![fake_ntfs_index_entry(44, "NTUSER.DAT", false)]);

    let mut queue = VecDeque::from([5u64]);
    let mut visited = HashSet::new();
    while let Some(dir_ref) = queue.pop_front() {
        if !visited.insert(dir_ref) {
            continue;
        }

        for action in mft_directory_index_backfill_actions(
            &mut path_map,
            dir_ref,
            indexes.remove(&dir_ref).unwrap_or_default(),
        ) {
            if action.is_dir && !visited.contains(&action.mft_ref) {
                queue.push_back(action.mft_ref);
            }
        }
    }

    assert_eq!(visited, HashSet::from([5, 42, 43]));
    assert_eq!(
        path_map.get("44"),
        Some(&(Some("43".to_string()), "NTUSER.DAT".to_string(), false))
    );
}

#[test]
fn ntfs_directory_index_parentage_corrects_existing_misparented_rows() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let ds_id = "ds-index-parentage";
    insert_staging_entry(&conn, "mft:4:5", ds_id);
    insert_staging_entry(&conn, "mft:4:42", ds_id);
    insert_staging_entry(&conn, "mft:4:43", ds_id);
    let mut path_map = HashMap::new();
    path_map.insert("5".to_string(), (None, "/".to_string(), true));
    path_map.insert(
        "42".to_string(),
        (Some("5".to_string()), "Users".to_string(), true),
    );
    path_map.insert(
        "43".to_string(),
        (Some("5".to_string()), "Liu Yang".to_string(), true),
    );

    let actions = mft_directory_index_backfill_actions(
        &mut path_map,
        42,
        vec![fake_ntfs_index_entry(43, "Liu Yang", true)],
    );

    assert_eq!(actions.len(), 1);
    assert_eq!(
        path_map.get("43"),
        Some(&(Some("42".to_string()), "Liu Yang".to_string(), true))
    );

    let deleted_records = HashSet::new();
    update_mft_staging_paths_via_sqlite(&conn, ds_id, 4, &path_map, &deleted_records).unwrap();
    let (path, parent_id): (String, Option<String>) = conn
        .query_row(
            "SELECT path, parent_id FROM file_entries WHERE id = 'mft:4:43'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(path, "Users/Liu Yang");
    assert_eq!(parent_id.as_deref(), Some("mft:4:42"));
}

#[test]
fn ntfs_directory_index_backfill_preserves_canonical_name_priority() {
    let mut path_map = HashMap::new();
    path_map.insert("5".to_string(), (None, "/".to_string(), true));
    path_map.insert(
        "42".to_string(),
        (Some("5".to_string()), "PROGRA~1".to_string(), true),
    );

    let actions = mft_directory_index_backfill_actions(
        &mut path_map,
        5,
        vec![
            fake_ntfs_index_entry(42, "Program Files", true),
            fake_ntfs_index_entry(42, "Program Files Hardlink", true),
        ],
    );

    assert_eq!(actions[0].name, "Program Files");
    assert_eq!(actions[1].name, "Program Files Hardlink");
    assert_eq!(
        path_map.get("42"),
        Some(&(Some("5".to_string()), "Program Files".to_string(), true))
    );
}

#[test]
fn ntfs_hardlink_representative_is_deterministic_across_parent_order() {
    fn select(order: &[(u64, &str)]) -> HashMap<String, (Option<String>, String, bool)> {
        let mut path_map = HashMap::from([
            ("5".to_string(), (None, "/".to_string(), true)),
            (
                "10".to_string(),
                (Some("5".to_string()), "Alpha".to_string(), true),
            ),
            (
                "20".to_string(),
                (Some("5".to_string()), "Beta".to_string(), true),
            ),
        ]);
        let mut representatives = HashMap::new();
        for (parent, name) in order {
            mft_directory_index_backfill_actions_with_representatives(
                &mut path_map,
                &mut representatives,
                *parent,
                vec![fake_ntfs_index_entry(99, *name, false)],
            );
        }
        path_map
    }

    let forward = select(&[(20, "zeta.txt"), (10, "alpha.txt")]);
    let reverse = select(&[(10, "alpha.txt"), (20, "zeta.txt")]);

    assert_eq!(forward.get("99"), reverse.get("99"));
    assert_eq!(
        forward.get("99"),
        Some(&(Some("10".to_string()), "alpha.txt".to_string(), false))
    );
}

#[test]
fn ntfs_directory_reference_sequence_rejects_reused_record() {
    let sequences = HashMap::from([(42, 7)]);

    assert!(directory_reference_sequence_matches(&sequences, 42, 7));
    assert!(!directory_reference_sequence_matches(&sequences, 42, 8));
    assert!(directory_reference_sequence_matches(&sequences, 99, 1));
}

#[test]
fn ntfs_directory_index_backfill_updates_persisted_mft_name() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let ds_id = "ds-index-name";
    insert_staging_entry(&conn, "mft:4:5", ds_id);
    insert_staging_entry(&conn, "mft:4:42", ds_id);
    let mut path_map = HashMap::from([
        ("5".to_string(), (None, "/".to_string(), true)),
        (
            "42".to_string(),
            (Some("5".to_string()), "PROGRA~1".to_string(), true),
        ),
    ]);

    mft_directory_index_backfill_actions(
        &mut path_map,
        5,
        vec![fake_ntfs_index_entry(42, "Program Files", true)],
    );
    update_mft_staging_paths_via_sqlite(&conn, ds_id, 4, &path_map, &HashSet::new()).unwrap();

    let name: String = conn
        .query_row(
            "SELECT name FROM file_entries WHERE id = 'mft:4:42'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name, "Program Files");
}

#[test]
fn mft_large_record_count_uses_sqlite_resolver() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let ds_id = "ds-mft-large";
    insert_staging_entry(&conn, "mft:9:5", ds_id);
    insert_staging_entry(&conn, "mft:9:42", ds_id);
    insert_staging_entry(&conn, "mft:9:43", ds_id);
    let mut path_map = HashMap::new();
    path_map.insert("5".to_string(), (None, "/".to_string(), true));
    path_map.insert(
        "42".to_string(),
        (Some("5".to_string()), "Windows".to_string(), true),
    );
    path_map.insert(
        "43".to_string(),
        (Some("42".to_string()), "notepad.exe".to_string(), false),
    );

    let deleted_records = HashSet::new();
    update_mft_staging_paths_via_sqlite(&conn, ds_id, 9, &path_map, &deleted_records).unwrap();

    let (path, parent_id): (String, Option<String>) = conn
        .query_row(
            "SELECT path, parent_id FROM file_entries WHERE id = 'mft:9:43'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(path, "Windows/notepad.exe");
    assert_eq!(parent_id.as_deref(), Some("mft:9:42"));
}

#[test]
fn ntfs_mft_update_uses_record_key_without_numeric_fallback() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    insert_staging_entry(&conn, "mft:8:0", "ds-mft");
    insert_staging_entry(&conn, "mft:8:bad-key", "ds-mft");

    let mut path_map = HashMap::new();
    path_map.insert("5".to_string(), (None, "/".to_string(), true));
    path_map.insert(
        "bad-key".to_string(),
        (Some("5".to_string()), "orphan.bin".to_string(), false),
    );

    let deleted_records = HashSet::new();
    update_mft_staging_paths_via_sqlite(&conn, "ds-mft", 8, &path_map, &deleted_records).unwrap();

    let bad: (String, Option<String>) = conn
        .query_row(
            "SELECT path, parent_id FROM file_entries WHERE id = 'mft:8:bad-key'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let zero_path: String = conn
        .query_row(
            "SELECT path FROM file_entries WHERE id = 'mft:8:0'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bad.0, "orphan.bin");
    assert_eq!(bad.1.as_deref(), Some("mft:8:5"));
    assert_eq!(zero_path, "");
}

#[test]
fn ntfs_mft_deleted_orphan_uses_deleted_orphans_path() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let ds_id = "ds-deleted-orphan";
    let records = vec![
        fake_mft_record(5, 5, ".", true),
        fake_deleted_mft_record(77, 999, "old.txt", false),
    ];
    let catalog = stage_mft_records_for_test(&conn, &records, ds_id, 2);
    catalog.update_staging_paths(&conn, ds_id, 2).unwrap();

    let (path, parent_id): (String, Option<String>) = conn
        .query_row(
            "SELECT path, parent_id FROM file_entries WHERE id = 'mft:2:77'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(path, "/$DeletedOrphans/77-old.txt");
    assert_eq!(parent_id.as_deref(), Some("mft:2:5"));
    let deleted: i64 = conn
        .query_row(
            "SELECT deleted FROM file_entries WHERE id = 'mft:2:77'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(deleted, 1);
}

#[test]
fn ntfs_mft_data_volume_without_windows_directories_is_accepted() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let ds_id = "ds-flat-mft";
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
             VALUES ('mft:3:5', NULL, ?1, '\\', '\\', 'directory')",
        rusqlite::params![ds_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
             VALUES ('mft:3:5662', 'mft:3:5', ?1, 'Media', 'Media', 'directory')",
        rusqlite::params![ds_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
             VALUES ('mft:3:109959', 'mft:3:5662', ?1, 'Media/video.mp4', 'video.mp4', 'file')",
        rusqlite::params![ds_id],
    )
    .unwrap();

    validate_mft_staging_shape(&conn, ds_id, 3).unwrap();
}

#[test]
fn ntfs_mft_orphaned_tree_is_rejected() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let ds_id = "ds-orphan-mft";
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
         VALUES ('mft:4:5', NULL, ?1, '\\', '\\', 'directory')",
        rusqlite::params![ds_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
         VALUES ('mft:4:42', 'mft:4:999', ?1, 'orphan.txt', 'orphan.txt', 'file')",
        rusqlite::params![ds_id],
    )
    .unwrap();

    let error = validate_mft_staging_shape(&conn, ds_id, 4).unwrap_err();
    assert!(error.to_string().contains("reachable=1"));
    assert!(error.to_string().contains("orphans=1"));
}

#[test]
fn ntfs_mft_stream_reads_split_runs() {
    let mut data = vec![0u8; 8192];
    data[1024..1536].fill(0xAA);
    data[4096..4608].fill(0xBB);
    let mut reader = FakeEvidenceReader::new(data);
    let mut out = vec![0u8; 1024];

    read_ntfs_mft_stream(&mut reader, 0, 512, &[(2, 1), (8, 1)], 0, &mut out).unwrap();

    assert!(out[..512].iter().all(|byte| *byte == 0xAA));
    assert!(out[512..].iter().all(|byte| *byte == 0xBB));
}

#[test]
#[ignore = "requires private Liu Yang BitLocker E01 path and recovery password"]
fn real_liuyang_bitlocker_reader_uses_mft_catalog_path() {
    let sample = std::env::var_os("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_E01")
        .map(PathBuf::from)
        .expect("set FORENSICS_BITLOCKER_PRIVATE_LIUYANG_E01");
    let credential = std::env::var("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_RECOVERY_PASSWORD")
        .map(Passphrase::new)
        .expect("set FORENSICS_BITLOCKER_PRIVATE_LIUYANG_RECOVERY_PASSWORD");

    let mut probe_reader = E01Reader::open(&sample).expect("open Liu Yang E01 read-only");
    let probe = crate::datasource_service::detect_image_filesystem(&mut probe_reader)
        .expect("probe Liu Yang E01");
    let partition = probe
        .partitions
        .iter()
        .find(|partition| partition.index == 5)
        .expect("find Liu Yang BitLocker partition 5");

    let evidence: Box<dyn EvidenceReader> =
        Box::new(E01Reader::open(&sample).expect("reopen Liu Yang E01 read-only"));
    let mut window = PartitionWindowReader::new(
        evidence,
        partition.offset,
        (partition.length > 0).then_some(partition.length),
    )
    .expect("open Liu Yang BitLocker partition window");
    let verified = unlock_volume_with_recovery_password(&mut window, &credential)
        .expect("unlock Liu Yang BitLocker partition");
    let registry = Arc::new(crate::bitlocker_runtime::BitLockerUnlockRegistry::default());
    registry
        .register_verified("case-liuyang", "source-liuyang", 5, verified)
        .expect("register Liu Yang BitLocker runtime");
    let plaintext = crate::bitlocker_runtime::open_registered_bitlocker_volume(
        Box::new(E01Reader::open(&sample).expect("reopen Liu Yang E01 for catalog")),
        partition.offset,
        (partition.length > 0).then_some(partition.length),
        "case-liuyang",
        "source-liuyang",
        5,
        &registry,
    )
    .expect("open Liu Yang plaintext reader");

    let mut diagnostic_plaintext = crate::bitlocker_runtime::open_registered_bitlocker_volume(
        Box::new(E01Reader::open(&sample).expect("reopen Liu Yang E01 for MFT diagnostics")),
        partition.offset,
        (partition.length > 0).then_some(partition.length),
        "case-liuyang",
        "source-liuyang",
        5,
        &registry,
    )
    .expect("open Liu Yang plaintext reader for MFT diagnostics");
    let mft = read_ntfs_mft_parameters_at(diagnostic_plaintext.as_mut(), 0)
        .expect("read Liu Yang MFT parameters");
    let run_bytes = mft
        .mft_data_runs
        .iter()
        .map(|(_, clusters)| clusters.saturating_mul(mft.cluster_size))
        .sum::<u64>();
    eprintln!(
        "Liu Yang BitLocker NTFS: partitionBytes={} mftDataBytes={} recordBytes={} totalRecords={} runs={} runBytes={}",
        partition.length,
        mft.mft_data_size,
        mft.record_size,
        mft.mft_data_size / u64::from(mft.record_size),
        mft.mft_data_runs.len(),
        run_bytes,
    );

    let temp = tempfile::TempDir::new().unwrap();
    let conn = staging::open_partition_staging(temp.path(), "source-liuyang", 5).unwrap();
    let started = Instant::now();
    let stats = enumerate_ntfs_reader_to_staging(
        &conn,
        plaintext,
        "source-liuyang",
        5,
        0,
        &AtomicBool::new(false),
    )
    .expect("enumerate Liu Yang NTFS catalog via plaintext reader");
    let elapsed = started.elapsed();
    let rows = stats.file_count + stats.dir_count;
    eprintln!(
        "Liu Yang BitLocker MFT catalog: rows={rows} files={} dirs={} directoryIndexFailures={} elapsedMs={} rowsPerSec={:.0}",
        stats.file_count,
        stats.dir_count,
        stats.directory_index_failures,
        elapsed.as_millis(),
        rows as f64 / elapsed.as_secs_f64().max(0.001),
    );

    assert!(stats.file_count > 0, "catalog must contain files");
    assert!(stats.dir_count > 0, "catalog must contain directories");
    let root_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries
             WHERE data_source_id = 'source-liuyang'
               AND partition_index = 5
               AND parent_id IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let root_child_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries
             WHERE data_source_id = 'source-liuyang'
               AND partition_index = 5
               AND parent_id = 'mft:5:5'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let reachable_count: i64 = conn
        .query_row(
            "WITH RECURSIVE reachable(id) AS (
                 SELECT id FROM file_entries
                  WHERE data_source_id = 'source-liuyang'
                    AND partition_index = 5
                    AND parent_id IS NULL
                 UNION ALL
                 SELECT child.id FROM file_entries child
                 JOIN reachable parent ON child.parent_id = parent.id
                  WHERE child.data_source_id = 'source-liuyang'
                    AND child.partition_index = 5
             )
             SELECT COUNT(*) FROM reachable",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let orphan_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries child
              WHERE child.data_source_id = 'source-liuyang'
                AND child.partition_index = 5
                AND child.parent_id IS NOT NULL
                AND NOT EXISTS (
                    SELECT 1 FROM file_entries parent WHERE parent.id = child.parent_id
                )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    eprintln!(
        "Liu Yang BitLocker tree: rootChildren={root_child_count} reachable={reachable_count} orphans={orphan_count} total={rows}"
    );
    assert_eq!(root_count, 1, "catalog must contain one NTFS root");
    assert_eq!(
        reachable_count as u64, rows,
        "all catalog rows must be reachable"
    );
    assert_eq!(orphan_count, 0, "catalog must not contain orphan rows");
    assert!(
        root_child_count > 0,
        "catalog must contain at least one reachable root child"
    );

    let (target_id, target_path, target_size, target_is_media): (String, String, u64, bool) = conn
        .query_row(
            "SELECT id, path, size,
                    CASE WHEN LOWER(name) GLOB '*.mp4'
                           OR LOWER(name) GLOB '*.mov'
                           OR LOWER(name) GLOB '*.avi'
                           OR LOWER(name) GLOB '*.mkv'
                           OR LOWER(name) GLOB '*.wmv'
                           OR LOWER(name) GLOB '*.webm'
                           OR LOWER(name) GLOB '*.mp3'
                           OR LOWER(name) GLOB '*.wav'
                           OR LOWER(name) GLOB '*.flac'
                           OR LOWER(name) GLOB '*.m4a'
                         THEN 1 ELSE 0 END AS is_media
               FROM file_entries
              WHERE data_source_id = 'source-liuyang'
                AND partition_index = 5
                AND entry_type = 'file'
                AND size > 0
              ORDER BY is_media DESC, size DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("find a non-empty Liu Yang BitLocker file");
    let target_inode = target_id
        .rsplit(':')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .expect("parse target MFT inode");
    let target_plaintext = crate::bitlocker_runtime::open_registered_bitlocker_volume(
        Box::new(E01Reader::open(&sample).expect("reopen Liu Yang E01 for range reads")),
        partition.offset,
        (partition.length > 0).then_some(partition.length),
        "case-liuyang",
        "source-liuyang",
        5,
        &registry,
    )
    .expect("open Liu Yang plaintext reader for range reads");
    let target_fs = NtfsReader::open(target_plaintext, 0).expect("open target NTFS reader");
    eprintln!(
        "Liu Yang BitLocker range target: id={target_id} size={target_size} media={target_is_media} path={target_path}"
    );
    for (label, offset) in [
        ("head", 0),
        ("middle", target_size / 2),
        ("tail", target_size.saturating_sub(64 * 1024)),
    ] {
        let started = Instant::now();
        let bytes = target_fs
            .read_file_range_by_inode(target_inode, offset, 64 * 1024)
            .unwrap_or_else(|error| panic!("read {label} file range at {offset}: {error}"));
        eprintln!(
            "Liu Yang BitLocker file range: label={label} offset={offset} bytes={} elapsedMs={}",
            bytes.len(),
            started.elapsed().as_millis()
        );
        assert!(!bytes.is_empty(), "{label} file range must return bytes");
    }

    let source_conn = persistence_sqlite::connection::open_in_memory().unwrap();
    source_conn
        .execute_batch(include_str!(
            "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
        ))
        .unwrap();
    source_conn
        .execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, partition_index)
             VALUES ('placeholder-5', NULL, 'source-liuyang',
                     '__partition_placeholder__/5/unlocked', 'Encrypted volume', 'directory', 5)",
            [],
        )
        .unwrap();
    drop(conn);
    let merge_conn = staging::open_partition_staging(temp.path(), "source-liuyang", 5).unwrap();
    persistence_sqlite::repositories::staging_repo::StagingRepo::merge_enum_staging_to_main(
        &source_conn,
        &merge_conn,
        "source-liuyang",
        5,
        "Partition 5 (NTFS, unlocked)",
    )
    .expect("fold Liu Yang staging catalog into the partition root");
    let merged_roots: Vec<(String, String)> = source_conn
        .prepare(
            "SELECT id, name FROM file_entries
             WHERE data_source_id = 'source-liuyang' AND parent_id IS NULL",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let merged_root_children: i64 = source_conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = 'placeholder-5'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        merged_roots,
        vec![(
            "placeholder-5".to_string(),
            "Partition 5 (NTFS, unlocked)".to_string()
        )]
    );
    assert!(merged_root_children > 0, "merged root must expose children");
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn real_e01_ntfs_mft_parameters_include_data_runs() {
    let sample = testing::fixtures::local_e01_fixture()
        .expect("set FORENSICS_E01_FIXTURE to run real E01 MFT test");
    let mut reader = E01Reader::open(&sample).unwrap();
    let probe = crate::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                crate::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("expected NTFS candidate");
    let partition = PartitionWork {
        // MBR fallback: use the offset-ordered index (same logic as pipeline.rs)
        index: ntfs.partition_index.unwrap_or(0),
        name: ntfs
            .partition_name
            .clone()
            .unwrap_or_else(|| "NTFS".to_string()),
        fs_kind: "ntfs".to_string(),
        fs: Box::new(FakeFsReader::new(
            "unused",
            0,
            Duration::ZERO,
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        )),
        source_path: sample,
        source_kind: "e01".to_string(),
        volume_offset: ntfs.offset,
    };

    let mut evidence_reader = open_partition_evidence_reader(&partition).unwrap();
    let params =
        read_ntfs_mft_parameters_at(&mut *evidence_reader, partition.volume_offset).unwrap();
    eprintln!(
        "mft cluster={} record_size={} data_size={} runs={:?}",
        params.mft_cluster,
        params.record_size,
        params.mft_data_size,
        params.mft_data_runs.iter().take(8).collect::<Vec<_>>()
    );
    assert!(
        !params.mft_data_runs.is_empty(),
        "real NTFS $MFT must expose non-resident data runs"
    );
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn real_e01_mft_parser_keeps_windows_parent_chain() {
    let sample = testing::fixtures::local_e01_fixture()
        .expect("set FORENSICS_E01_FIXTURE to run real E01 MFT test");
    let mut reader = E01Reader::open(&sample).unwrap();
    let probe = crate::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                crate::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("expected NTFS candidate");
    let partition = PartitionWork {
        // MBR fallback: use the offset-ordered index (same logic as pipeline.rs)
        index: ntfs.partition_index.unwrap_or(0),
        name: "NTFS".to_string(),
        fs_kind: "ntfs".to_string(),
        fs: Box::new(FakeFsReader::new(
            "unused",
            0,
            Duration::ZERO,
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        )),
        source_path: sample,
        source_kind: "e01".to_string(),
        volume_offset: ntfs.offset,
    };
    let mut evidence_reader = open_partition_evidence_reader(&partition).unwrap();
    let params =
        read_ntfs_mft_parameters_at(&mut *evidence_reader, partition.volume_offset).unwrap();
    let scanner = MftScanner::new(
        params.volume_offset,
        params.mft_cluster,
        params.cluster_size,
        params.record_size,
        params.bytes_per_sector,
        params.mft_data_size,
    );
    let mut buf = vec![0u8; scanner.total_records() as usize * scanner.record_size() as usize];
    read_ntfs_mft_stream(
        &mut *evidence_reader,
        params.volume_offset,
        params.cluster_size,
        &params.mft_data_runs,
        0,
        &mut buf,
    )
    .unwrap();
    let records = scanner.parse_chunk(&buf, 0, scanner.total_records());
    let windows = records
        .iter()
        .filter(|record| record.name.eq_ignore_ascii_case("Windows"))
        .take(8)
        .map(|record| {
            (
                record.record_number,
                record.parent_ref,
                record.is_dir,
                record.name.clone(),
            )
        })
        .collect::<Vec<_>>();
    let system32 = records
        .iter()
        .filter(|record| record.name.eq_ignore_ascii_case("System32"))
        .take(8)
        .map(|record| {
            (
                record.record_number,
                record.parent_ref,
                record.is_dir,
                record.name.clone(),
            )
        })
        .collect::<Vec<_>>();
    let parent_records = system32
        .iter()
        .filter_map(|(_, parent, _, _)| {
            records
                .iter()
                .find(|record| record.record_number == *parent)
                .map(|record| {
                    (
                        record.record_number,
                        record.parent_ref,
                        record.is_dir,
                        record.is_valid,
                        record.name.clone(),
                    )
                })
        })
        .collect::<Vec<_>>();
    eprintln!("Windows records: {windows:?}");
    eprintln!("System32 records: {system32:?}");
    eprintln!("System32 parent records: {parent_records:?}");
    let ntfs = NtfsReader::open(
        open_partition_evidence_reader(&partition).unwrap(),
        ntfs.offset,
    )
    .unwrap();
    let root_entries = ntfs.list_root_directory_entries().unwrap();
    let windows_record = root_entries
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case("Windows") && entry.is_dir)
        .map(|entry| entry.mft_ref)
        .expect("root index must expose Windows directory");
    assert!(
        system32
            .iter()
            .any(|(_, parent, is_dir, _)| *parent == windows_record && *is_dir),
        "expected System32 directory under Windows record {windows_record}"
    );
}

#[test]
fn ntfs_mft_fast_path_fallback_records_warning() {
    let tmp = tempfile::TempDir::new().unwrap();
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    let mut partition = fake_partition_work(0, 3, Duration::ZERO, active_lists, max_active_lists);
    partition.fs_kind = "ntfs".to_string();
    partition.source_path = tmp.path().join("missing.raw");

    let result = enumerate_single_partition(
        tmp.path(),
        "ds-mft-fallback",
        partition,
        &AtomicBool::new(false),
        None,
    );

    assert!(result.error.is_none());
    assert_eq!(result.file_count, 3);
    assert_eq!(result.dir_count, 1);
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].starts_with("MFT fast path fallback:"));
    let conn = staging::open_partition_staging(tmp.path(), "ds-mft-fallback", 0).unwrap();
    let warning = staging::get_staging_meta(&conn, "mft_fallback_warning")
        .unwrap()
        .unwrap();
    assert!(!warning.trim().is_empty());
    conn.execute(
        "INSERT INTO staging_meta (key, value) VALUES ('post_fallback_write', 'ok')",
        [],
    )
    .unwrap();
}

#[test]
fn parallel_enum_respects_max_workers() {
    let tmp = tempfile::TempDir::new().unwrap();
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    let partitions = (0..4)
        .map(|index| {
            fake_partition_work(
                index,
                1,
                Duration::from_millis(25),
                active_lists.clone(),
                max_active_lists.clone(),
            )
        })
        .collect();

    let results = enumerate_partitions_parallel(
        tmp.path(),
        &DataSourceId("ds-max-workers".to_string()),
        partitions,
        1,
        Arc::new(AtomicBool::new(false)),
        &|_, _, _| {},
    )
    .unwrap();

    assert_eq!(results.len(), 4);
    assert!(results.iter().all(|result| result.error.is_none()));
    assert_eq!(max_active_lists.load(Ordering::SeqCst), 1);
}

#[test]
fn parallel_enum_serializes_e01_partition_reads() {
    let tmp = tempfile::TempDir::new().unwrap();
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    let partitions = (0..3)
        .map(|index| {
            let mut work = fake_partition_work(
                index,
                1,
                Duration::from_millis(20),
                active_lists.clone(),
                max_active_lists.clone(),
            );
            work.source_kind = "e01".to_string();
            work
        })
        .collect();

    let results = enumerate_partitions_parallel(
        tmp.path(),
        &DataSourceId("ds-e01-serial".to_string()),
        partitions,
        4,
        Arc::new(AtomicBool::new(false)),
        &|_, _, _| {},
    )
    .unwrap();

    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|result| result.error.is_none()));
    assert_eq!(max_active_lists.load(Ordering::SeqCst), 1);
}

#[test]
fn parallel_enum_returns_results_in_partition_order() {
    let tmp = tempfile::TempDir::new().unwrap();
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    let partitions = vec![
        fake_partition_work(
            2,
            1,
            Duration::ZERO,
            active_lists.clone(),
            max_active_lists.clone(),
        ),
        fake_partition_work(
            0,
            1,
            Duration::from_millis(30),
            active_lists.clone(),
            max_active_lists.clone(),
        ),
        fake_partition_work(
            1,
            1,
            Duration::from_millis(5),
            active_lists,
            max_active_lists,
        ),
    ];

    let results = enumerate_partitions_parallel(
        tmp.path(),
        &DataSourceId("ds-result-order".to_string()),
        partitions,
        3,
        Arc::new(AtomicBool::new(false)),
        &|_, _, _| {},
    )
    .unwrap();

    assert_eq!(
        results
            .iter()
            .map(|result| result.index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn parallel_enum_uses_external_cancel_token() {
    let tmp = tempfile::TempDir::new().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = cancel.clone();
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    let partitions = vec![fake_partition_work(
        0,
        10,
        Duration::from_millis(75),
        active_lists,
        max_active_lists,
    )];

    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        cancel_for_thread.store(true, Ordering::Relaxed);
    });

    let results = enumerate_partitions_parallel(
        tmp.path(),
        &DataSourceId("ds-cancel".to_string()),
        partitions,
        1,
        cancel,
        &|_, _, _| {},
    )
    .unwrap();
    canceller.join().unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_count, 0);
    assert_eq!(results[0].error.as_deref(), Some("Cancelled"));
}

#[test]
fn parallel_enum_cancel_leaves_unstarted_partitions_without_failure_results() {
    let tmp = tempfile::TempDir::new().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = cancel.clone();
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    let partitions = (0..3)
        .map(|index| {
            fake_partition_work(
                index,
                10,
                Duration::from_millis(75),
                active_lists.clone(),
                max_active_lists.clone(),
            )
        })
        .collect();

    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        cancel_for_thread.store(true, Ordering::Relaxed);
    });

    let results = enumerate_partitions_parallel(
        tmp.path(),
        &DataSourceId("ds-cancel-pending".to_string()),
        partitions,
        1,
        cancel,
        &|_, _, _| {},
    )
    .unwrap();
    canceller.join().unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].index, 0);
    assert_eq!(results[0].error.as_deref(), Some("Cancelled"));
}

#[test]
fn recursive_enum_cancel_rolls_back_current_staging_transaction() {
    let tmp = tempfile::TempDir::new().unwrap();
    let cancel = AtomicBool::new(false);
    let progress_seen = AtomicUsize::new(0);
    let active_lists = Arc::new(AtomicUsize::new(0));
    let max_active_lists = Arc::new(AtomicUsize::new(0));
    let partition = fake_partition_work(0, 5_001, Duration::ZERO, active_lists, max_active_lists);

    let result = enumerate_single_partition(
        tmp.path(),
        "ds-cancel-rollback",
        partition,
        &cancel,
        Some(&|_, _| {
            progress_seen.fetch_add(1, Ordering::SeqCst);
            cancel.store(true, Ordering::Relaxed);
        }),
    );

    assert_eq!(result.error.as_deref(), Some("Cancelled"));
    assert!(progress_seen.load(Ordering::SeqCst) > 0);
    let conn = staging::open_partition_staging(tmp.path(), "ds-cancel-rollback", 0).unwrap();
    assert_eq!(staging::staging_db_row_count(&conn).unwrap(), 0);
    assert_eq!(
        staging::get_staging_meta(&conn, "status")
            .unwrap()
            .as_deref(),
        Some("failed")
    );
}

#[test]
fn progress_backpressure_does_not_block_worker() {
    let tmp = tempfile::TempDir::new().unwrap();
    let started = Instant::now();
    let progress_events = Arc::new(AtomicUsize::new(0));
    let progress_events_for_cb = progress_events.clone();

    let results = enumerate_partitions_parallel(
        tmp.path(),
        &DataSourceId("ds-progress-backpressure".to_string()),
        fake_partitions(1, 10_000, Duration::ZERO),
        1,
        Arc::new(AtomicBool::new(false)),
        &|_, _, _| {
            progress_events_for_cb.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
        },
    )
    .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].error.is_none());
    assert!(progress_events.load(Ordering::SeqCst) > 0);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "progress backpressure blocked enumeration for {:?}",
        started.elapsed()
    );
}

#[test]
fn single_partition_emits_entry_progress() {
    let tmp = tempfile::TempDir::new().unwrap();
    let progress = Arc::new(Mutex::new(Vec::<(u32, String)>::new()));
    let progress_for_cb = progress.clone();

    let results = enumerate_partitions_parallel(
        tmp.path(),
        &DataSourceId("ds-single-heartbeat".to_string()),
        fake_partitions(1, 10_000, Duration::ZERO),
        4,
        Arc::new(AtomicBool::new(false)),
        &|_, pct, detail| {
            progress_for_cb
                .lock()
                .unwrap()
                .push((pct, detail.to_string()));
        },
    )
    .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].error.is_none());
    let progress = progress.lock().unwrap_or_else(|e| e.into_inner());
    assert!(progress.iter().any(|(pct, detail)| {
        *pct > 0 && *pct < 100 && detail.starts_with("Partition 0:") && detail.ends_with("entries")
    }));
}

#[test]
fn test_staging_db_insert_and_count() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();

    conn.execute_batch("BEGIN TRANSACTION").unwrap();
    for i in 0..5 {
        conn.execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type) VALUES (?1, ?2, ?3, ?4, 'File')",
                rusqlite::params![
                    format!("f{}", i),
                    "ds-1",
                    format!("/test/file{}.txt", i),
                    format!("file{}.txt", i),
                ],
            )
            .unwrap();
    }
    conn.execute_batch("COMMIT").unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 5);
}

#[test]
fn test_staging_db_preserves_data() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();

    conn.execute(
            "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size, ext)
             VALUES (?1, ?2, ?3, ?4, ?5, 'File', 4096, 'pdf')",
            rusqlite::params!["data-test", "parent-1", "ds-x", "/root/doc.pdf", "doc.pdf"],
        )
        .unwrap();

    let (path, name, entry_type): (String, String, String) = conn
        .query_row(
            "SELECT path, name, entry_type FROM file_entries WHERE id = 'data-test'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(path, "/root/doc.pdf");
    assert_eq!(name, "doc.pdf");
    assert_eq!(entry_type, "File");
}
