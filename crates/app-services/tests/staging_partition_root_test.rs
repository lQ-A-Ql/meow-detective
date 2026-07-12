mod staging_support;

use app_services::staging::{merge_all_staging_to_main, open_partition_staging};
use staging_support::{
    create_main_file_entries_table, first_level_roots, insert_staging_row, seed_placeholder,
    single_done_manifest,
};

#[test]
fn staging_partition_root_lookup_does_not_collide_on_digit_prefix() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-glob-collision";
    let root_12 = seed_placeholder(&main, ds_id, 12, "Partition 12 (NTFS)");
    let staging = open_partition_staging(tmp.path(), ds_id, 1).unwrap();
    insert_staging_row(&staging, ds_id, "f1", None, "file.txt", "file");
    drop(staging);
    let mut manifest = single_done_manifest(ds_id, "Partition 1 (NTFS)");
    manifest.partitions[0].index = 1;

    merge_all_staging_to_main(&main, tmp.path(), ds_id, &manifest, None).unwrap();
    let parent: String = main
        .query_row(
            "SELECT parent_id FROM file_entries WHERE id = 'f1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(parent, root_12);
    let path_12: String = main
        .query_row(
            "SELECT path FROM file_entries WHERE id = ?1",
            [&root_12],
            |row| row.get(0),
        )
        .unwrap();
    assert!(path_12.starts_with("__partition_placeholder__/12/"));
}

#[test]
fn staging_partition_root_folds_null_parent_synthetic_root() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-fold-null";
    let placeholder = seed_placeholder(&main, ds_id, 0, "Partition 0 (NTFS)");
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    insert_staging_row(&staging, ds_id, "root5", None, "\\", "directory");
    insert_staging_row(
        &staging,
        ds_id,
        "windows",
        Some("root5"),
        "Windows",
        "directory",
    );
    drop(staging);

    merge_all_staging_to_main(
        &main,
        tmp.path(),
        ds_id,
        &single_done_manifest(ds_id, "Partition 0 (NTFS)"),
        None,
    )
    .unwrap();
    assert_eq!(
        first_level_roots(&main),
        vec!["Partition 0 (NTFS)".to_string()]
    );
    let parent: String = main
        .query_row(
            "SELECT parent_id FROM file_entries WHERE id = 'windows'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(parent, placeholder);
}

#[test]
fn staging_partition_root_folds_self_referential_root() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-fold-self";
    let placeholder = seed_placeholder(&main, ds_id, 0, "Partition 0 (NTFS)");
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    insert_staging_row(
        &staging,
        ds_id,
        "selfroot",
        Some("selfroot"),
        ".",
        "directory",
    );
    insert_staging_row(
        &staging,
        ds_id,
        "docs",
        Some("selfroot"),
        "Docs",
        "directory",
    );
    drop(staging);

    merge_all_staging_to_main(
        &main,
        tmp.path(),
        ds_id,
        &single_done_manifest(ds_id, "Partition 0 (NTFS)"),
        None,
    )
    .unwrap();
    let parent: String = main
        .query_row(
            "SELECT parent_id FROM file_entries WHERE id = 'docs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(parent, placeholder);
}

#[test]
fn staging_partition_root_folds_slash_root_and_synthesizes_placeholder() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-fold-slash";
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    insert_staging_row(&staging, ds_id, "root", None, "/", "directory");
    insert_staging_row(&staging, ds_id, "boot", Some("root"), "boot.ini", "file");
    drop(staging);

    merge_all_staging_to_main(
        &main,
        tmp.path(),
        ds_id,
        &single_done_manifest(ds_id, "Partition 0 (FAT)"),
        None,
    )
    .unwrap();
    assert_eq!(
        first_level_roots(&main),
        vec!["Partition 0 (FAT)".to_string()]
    );
    let root_count: i64 = main
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE name IN ('/', '\\', '.')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(root_count, 0);
}

#[test]
fn staging_partition_root_keeps_real_fat_top_level_entries() {
    let tmp = tempfile::TempDir::new().unwrap();
    let main = persistence_sqlite::connection::open_in_memory().unwrap();
    create_main_file_entries_table(&main);
    let ds_id = "ds-fat-efi";
    let placeholder = seed_placeholder(&main, ds_id, 0, "Partition 0 (FAT)");
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    insert_staging_row(&staging, ds_id, "efi", None, "EFI", "directory");
    insert_staging_row(&staging, ds_id, "boot", Some("efi"), "Boot", "directory");
    drop(staging);

    merge_all_staging_to_main(
        &main,
        tmp.path(),
        ds_id,
        &single_done_manifest(ds_id, "Partition 0 (FAT)"),
        None,
    )
    .unwrap();
    let efi_parent: String = main
        .query_row(
            "SELECT parent_id FROM file_entries WHERE id = 'efi'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(efi_parent, placeholder);
    let efi_count: i64 = main
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE id = 'efi'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(efi_count, 1);
}
