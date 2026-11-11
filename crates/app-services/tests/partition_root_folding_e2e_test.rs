//! Stage D end-to-end closure test for the partition-root folding fix.
//!
//! This exercises the full A+B+C chain without any external evidence fixture:
//!
//!   1. (A) Placeholder roots are inserted with `partition_index`-encoded paths.
//!   2. (B) Per-partition staging DBs — one NTFS-style with a synthetic `\` root,
//!      one FAT-style with NULL-parent top-level directories (e.g. `EFI`) — are
//!      merged via the real `merge_all_staging_to_main`, folding synthetic roots
//!      into the placeholder and re-parenting real top-level entries.
//!   3. (C) `get_file_tree_real_with_visibility` returns a stable first level of
//!      partition roots with NO bare `\` / `EFI` leaking to depth 0.
//!
//! The assertion that the first tree level contains exactly the partition roots
//! (and never a raw filesystem root) is the regression guard for the bug recorded
//! in docs/archive/status/2026-06/pause-status-2026-06-11-file-tree-sorting.md.

use app_services::{file_service, staging};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use rusqlite::{params, Connection};
use tempfile::TempDir;

const DS_ID: &str = "ds-e2e-folding";

fn seed_source_connection(case_root: &std::path::Path) -> Connection {
    let conn = app_services::source_db::open_source_db(case_root, &DataSourceId(DS_ID.to_string()))
        .unwrap();
    conn.execute(
        "INSERT INTO data_sources
         (id, case_id, name, kind, source_path, imported_at)
         VALUES (?1, 'case-1', ?1, 'e01', 'disk.E01',
                 '2026-01-01T00:00:00Z')",
        [DS_ID],
    )
    .unwrap();
    conn
}

fn seed_partitions_table(case_root: &std::path::Path) {
    let storage = app_services::source_db::open_fjall_source_read_write(case_root, DS_ID).unwrap();
    storage
        .partition_replace_by_source(
            DS_ID,
            &[
                persistence_fjall::PartitionRecord {
                    id: "part-0".to_string(),
                    data_source_id: DS_ID.to_string(),
                    partition_index: 0,
                    name: "Basic data partition".to_string(),
                    kind_label: "Basic data".to_string(),
                    status: "supported".to_string(),
                    type_guid: None,
                    offset: 0,
                    length: 1024,
                    filesystem: Some("NTFS".to_string()),
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
                },
                persistence_fjall::PartitionRecord {
                    id: "part-1".to_string(),
                    data_source_id: DS_ID.to_string(),
                    partition_index: 1,
                    name: "EFI system partition".to_string(),
                    kind_label: "EFI System".to_string(),
                    status: "supported".to_string(),
                    type_guid: None,
                    offset: 2048,
                    length: 1024,
                    filesystem: Some("FAT".to_string()),
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
                },
            ],
            persistence_fjall::PersistMode::SyncAll,
        )
        .unwrap();
    storage.partition_projection_publish(DS_ID).unwrap();
}

fn source_storage(
    case_root: &std::path::Path,
) -> app_services::source_db::FjallReadWriteSourceStorageHandle {
    app_services::source_db::open_fjall_source_read_write(case_root, DS_ID).unwrap()
}

fn publish_source_projection(conn: &Connection) {
    let store = persistence_source::detail::open_store_for_write(conn)
        .unwrap()
        .unwrap();
    persistence_fjall::seal_source_generation(&store, DS_ID, "fixture", &"a".repeat(64)).unwrap();
    let key = persistence_fjall::encode_key(
        "projection",
        &[persistence_fjall::KeyPart::Text(DS_ID.to_string())],
    )
    .unwrap();
    let marker = persistence_fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{DS_ID}"),
        DS_ID,
        1,
        DS_ID.as_bytes().to_vec(),
    );
    store
        .write_envelope(
            persistence_fjall::KeyspaceName::Meta,
            key,
            &marker,
            persistence_fjall::PersistMode::SyncAll,
        )
        .unwrap();
}

fn catalog_records(
    storage: &app_services::source_db::FjallReadWriteSourceStorageHandle,
) -> Vec<persistence_fjall::FileEntryRecord> {
    let snapshot = storage.snapshot();
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = storage
            .catalog_list_all(
                &snapshot,
                DS_ID,
                cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                512,
            )
            .unwrap();
        records.extend(page.records);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    records
}

/// NTFS-style staging DB: a synthetic `\` root (MFT record 5) with children
/// pointing at it, plus a nested file. This is the shape the MFT enumerator
/// writes — the `\` root must be folded into the partition placeholder.
fn seed_ntfs_staging(case_root: &std::path::Path) {
    let conn = staging::open_partition_staging(case_root, DS_ID, 0).unwrap();
    // Synthetic filesystem root.
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
         VALUES ('mft:0:5', NULL, ?1, '', '\\', 'directory')",
        params![DS_ID],
    )
    .unwrap();
    // Top-level directory under the synthetic root.
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
         VALUES ('mft:0:64', 'mft:0:5', ?1, 'Windows', 'Windows', 'directory')",
        params![DS_ID],
    )
    .unwrap();
    // Nested file.
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size)
         VALUES ('mft:0:80', 'mft:0:64', ?1, 'Windows/notepad.exe', 'notepad.exe', 'file', 4096)",
        params![DS_ID],
    )
    .unwrap();
}

/// FAT-style staging DB: NULL-parent top-level entries (e.g. `EFI`) with NO
/// synthetic root row. These real top-level directories must be re-parented to
/// the partition placeholder and kept (not dropped).
fn seed_fat_staging(case_root: &std::path::Path) {
    let conn = staging::open_partition_staging(case_root, DS_ID, 1).unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
         VALUES ('fat-efi', NULL, ?1, 'EFI', 'EFI', 'directory')",
        params![DS_ID],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
         VALUES ('fat-boot', 'fat-efi', ?1, 'EFI/Boot', 'Boot', 'directory')",
        params![DS_ID],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size)
         VALUES ('fat-bootx64', 'fat-boot', ?1, 'EFI/Boot/bootx64.efi', 'bootx64.efi', 'file', 512)",
        params![DS_ID],
    )
    .unwrap();
}

fn make_done_partition(index: usize, name: &str, file_count: u64) -> staging::PartitionEntry {
    staging::PartitionEntry {
        index,
        name: name.to_string(),
        fs_kind: if index == 0 { "Ntfs" } else { "Fat" }.to_string(),
        staging_db: format!("enum_partition_{}.db", index),
        status: staging::PartitionStatus::Done,
        file_count,
        dir_count: 0,
        total_size: 0,
        last_path: None,
        completed_at: None,
        error: None,
    }
}

#[test]
fn full_merge_yields_only_partition_roots_no_bare_fs_roots() {
    let tmp = TempDir::new().unwrap();
    let case_root = tmp.path();
    let main_conn = seed_source_connection(case_root);
    seed_partitions_table(case_root);
    let storage = source_storage(case_root);

    // (A) Insert index-bound placeholder roots, exactly as the import pipeline does.
    file_service::insert_partition_placeholder_root(
        &main_conn,
        &domain::DataSourceId(DS_ID.to_string()),
        0,
        "Partition 0 (NTFS)",
        "queued",
    )
    .unwrap();
    file_service::insert_partition_placeholder_root(
        &main_conn,
        &domain::DataSourceId(DS_ID.to_string()),
        1,
        "Partition 1 (FAT)",
        "queued",
    )
    .unwrap();

    // Build per-partition staging DBs.
    seed_ntfs_staging(case_root);
    seed_fat_staging(case_root);

    let mut manifest = staging::StagingManifest::create(DS_ID, "/disk.E01", "E01");
    manifest
        .partitions
        .push(make_done_partition(0, "Partition 0 (NTFS)", 1));
    manifest
        .partitions
        .push(make_done_partition(1, "Partition 1 (FAT)", 1));

    // (B) Run the real merge.
    staging::merge_all_staging_to_main_with_catalog(
        &main_conn, case_root, DS_ID, &manifest, &storage, None, None,
    )
    .unwrap();
    publish_source_projection(&main_conn);

    // (C) Build the tree and assert the first level is stable partition roots.
    let tree = file_service::get_file_tree_real_with_visibility(&main_conn, false).unwrap();

    let root_names: Vec<&str> = tree.iter().map(|node| node.name.as_str()).collect();

    // No raw filesystem root must ever appear at depth 0.
    assert!(
        !root_names.contains(&"\\") && !root_names.contains(&"/"),
        "bare filesystem root leaked into tree first level: {:?}",
        root_names
    );
    assert!(
        !root_names.contains(&"EFI"),
        "FAT top-level directory leaked into tree first level: {:?}",
        root_names
    );

    // Exactly the two partition roots, both flagged as partition nodes.
    assert_eq!(
        tree.len(),
        2,
        "expected two partition roots: {:?}",
        root_names
    );
    assert!(root_names.contains(&"Partition 0 (NTFS)"));
    assert!(root_names.contains(&"Partition 1 (FAT)"));
    for node in &tree {
        assert_eq!(
            node.node_type.as_deref(),
            Some("partition"),
            "root {} should be a partition node",
            node.name
        );
        assert_eq!(node.depth, 0);
        assert!(
            node.has_children,
            "partition root {} should have children",
            node.name
        );
    }

    // The NTFS partition root's children must include the re-parented `Windows`
    // dir (the synthetic `\` is gone, but its child survived under the root).
    let ntfs_root = tree
        .iter()
        .find(|n| n.name == "Partition 0 (NTFS)")
        .unwrap();
    let ntfs_children =
        file_service::get_file_children_lazy(&main_conn, &ntfs_root.id, 0, 500).unwrap();
    let ntfs_child_names: Vec<&str> = ntfs_children
        .children
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert!(
        ntfs_child_names.contains(&"Windows"),
        "NTFS root should adopt the synthetic root's child Windows: {:?}",
        ntfs_child_names
    );

    // The FAT partition root's children must include the kept `EFI` directory.
    let fat_root = tree.iter().find(|n| n.name == "Partition 1 (FAT)").unwrap();
    let fat_children =
        file_service::get_file_children_lazy(&main_conn, &fat_root.id, 0, 500).unwrap();
    let fat_child_names: Vec<&str> = fat_children
        .children
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert!(
        fat_child_names.contains(&"EFI"),
        "FAT root should keep the real top-level EFI directory as a child: {:?}",
        fat_child_names
    );

    // The synthetic `\` root must not exist anywhere in the merged main DB.
    let records = catalog_records(&storage);
    let bare_root_count = records.iter().filter(|record| record.name == "\\").count();
    assert_eq!(
        bare_root_count, 0,
        "synthetic `\\` root must be folded away"
    );

    // No placeholder marker paths must survive merge (all promoted to real roots).
    let leftover_placeholders = records
        .iter()
        .filter(|record| record.path.starts_with("__partition_placeholder__/"))
        .count();
    assert_eq!(
        leftover_placeholders, 0,
        "placeholder roots should be promoted to real partition roots after merge"
    );
}

/// Resume scenario (T-D2): merging the same manifest twice must be idempotent and
/// must not re-introduce bare roots or duplicate partition roots.
#[test]
fn repeated_merge_is_idempotent_and_stays_folded() {
    let tmp = TempDir::new().unwrap();
    let case_root = tmp.path();
    let main_conn = seed_source_connection(case_root);
    seed_partitions_table(case_root);
    let storage = source_storage(case_root);

    file_service::insert_partition_placeholder_root(
        &main_conn,
        &domain::DataSourceId(DS_ID.to_string()),
        0,
        "Partition 0 (NTFS)",
        "queued",
    )
    .unwrap();
    file_service::insert_partition_placeholder_root(
        &main_conn,
        &domain::DataSourceId(DS_ID.to_string()),
        1,
        "Partition 1 (FAT)",
        "queued",
    )
    .unwrap();

    seed_ntfs_staging(case_root);
    seed_fat_staging(case_root);

    let mut manifest = staging::StagingManifest::create(DS_ID, "/disk.E01", "E01");
    manifest
        .partitions
        .push(make_done_partition(0, "Partition 0 (NTFS)", 1));
    manifest
        .partitions
        .push(make_done_partition(1, "Partition 1 (FAT)", 1));

    staging::merge_all_staging_to_main_with_catalog(
        &main_conn, case_root, DS_ID, &manifest, &storage, None, None,
    )
    .unwrap();
    publish_source_projection(&main_conn);
    // Second pass: staging DBs are now marked merged, so this is a no-op merge.
    staging::merge_all_staging_to_main_with_catalog(
        &main_conn, case_root, DS_ID, &manifest, &storage, None, None,
    )
    .unwrap();
    publish_source_projection(&main_conn);

    let tree = file_service::get_file_tree_real_with_visibility(&main_conn, false).unwrap();
    let root_names: Vec<&str> = tree.iter().map(|node| node.name.as_str()).collect();

    assert_eq!(
        tree.len(),
        2,
        "idempotent merge must not duplicate partition roots: {:?}",
        root_names
    );
    assert!(!root_names
        .iter()
        .any(|name| *name == "\\" || *name == "EFI"));
}

#[test]
fn redirected_lvm_placeholder_root_can_be_removed_on_resume_repair() {
    let tmp = TempDir::new().unwrap();
    let main_conn = seed_source_connection(tmp.path());
    let ds_id = DataSourceId(DS_ID.to_string());

    let storage = app_services::source_db::open_fjall_source_read_write(tmp.path(), DS_ID).unwrap();
    storage
        .partition_replace_by_source(
            DS_ID,
            &[
                persistence_fjall::PartitionRecord {
                    id: "part-lvm-pool".to_string(),
                    data_source_id: DS_ID.to_string(),
                    partition_index: 1,
                    name: "Linux LVM".to_string(),
                    kind_label: "LVM".to_string(),
                    status: "supported".to_string(),
                    type_guid: None,
                    offset: 1_048_576,
                    length: 1024,
                    filesystem: Some("LVM".to_string()),
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
                },
                persistence_fjall::PartitionRecord {
                    id: "part-root-lv".to_string(),
                    data_source_id: DS_ID.to_string(),
                    partition_index: 2,
                    name: "cl/root".to_string(),
                    kind_label: "XFS".to_string(),
                    status: "supported".to_string(),
                    type_guid: None,
                    offset: 1_048_576,
                    length: 1024,
                    filesystem: Some("XFS".to_string()),
                    unlock_hint: None,
                    lvm_vg_uuid: Some("vg".to_string()),
                    lvm_vg_name: Some("cl".to_string()),
                    lvm_lv_uuid: Some("lv".to_string()),
                    lvm_lv_name: Some("root".to_string()),
                    lvm_pv_offsets_json: Some("[1048576]".to_string()),
                    lvm_pv_sources_json: None,
                    gpt_disk_guid: None,
                    gpt_partition_uuid: None,
                    filesystem_uuid: None,
                },
            ],
            persistence_fjall::PersistMode::SyncAll,
        )
        .unwrap();
    storage.partition_projection_publish(DS_ID).unwrap();

    file_service::insert_partition_placeholder_root(
        &main_conn,
        &ds_id,
        1,
        "Partition 1 (LVM)",
        "queued",
    )
    .unwrap();
    persistence_source::repositories::file_repo::FileRepo::new(&main_conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId("root-lv".into()),
            parent_id: None,
            data_source_id: ds_id.clone(),
            path: String::new(),
            name: "Partition 2 (XFS) - cl/root".into(),
            entry_type: EntryType::Directory,
            size: None,
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .unwrap();

    let removed = file_service::remove_partition_placeholder_root(&main_conn, &ds_id, 1).unwrap();

    assert_eq!(removed, 1);
    let roots = persistence_source::repositories::file_repo::FileRepo::new(&main_conn)
        .find_roots(&ds_id)
        .unwrap();
    assert!(
        roots
            .iter()
            .all(|entry| !entry.path.starts_with("__partition_placeholder__/1/")),
        "redirected LVM placeholder should be removed from roots: {roots:?}"
    );
    assert!(
        roots
            .iter()
            .any(|entry| entry.id == FileEntryId("root-lv".to_string())
                && entry.entry_type == EntryType::Directory),
        "real LV root must survive placeholder cleanup: {roots:?}"
    );
}
