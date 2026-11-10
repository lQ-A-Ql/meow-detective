mod staging_support;

use app_services::staging::{merge_all_staging_to_main_with_catalog, open_partition_staging};
use staging_support::{
    first_level_roots, insert_staging_row, seed_placeholder, single_done_manifest,
    NativeSourceFixture,
};

#[test]
fn staging_partition_root_lookup_does_not_collide_on_digit_prefix() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-glob-collision";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
    let root_12 = seed_placeholder(&source.conn, ds_id, 12, "Partition 12 (NTFS)");
    let staging = open_partition_staging(tmp.path(), ds_id, 1).unwrap();
    insert_staging_row(&staging, ds_id, "f1", None, "file.txt", "file");
    drop(staging);
    let mut manifest = single_done_manifest(ds_id, "Partition 1 (NTFS)");
    manifest.partitions[0].index = 1;

    merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &manifest,
        &source.storage,
        None,
        None,
    )
    .unwrap();
    let records = source.catalog_records();
    let parent = records
        .iter()
        .find(|record| record.id == "f1")
        .and_then(|record| record.parent_id.as_deref())
        .unwrap();
    assert_ne!(parent, root_12);
    let root = records.iter().find(|record| record.id == root_12).unwrap();
    assert!(root.path.starts_with("__partition_placeholder__/12/"));
}

#[test]
fn staging_partition_root_folds_null_parent_synthetic_root() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-fold-null";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
    let placeholder = seed_placeholder(&source.conn, ds_id, 0, "Partition 0 (NTFS)");
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

    merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &single_done_manifest(ds_id, "Partition 0 (NTFS)"),
        &source.storage,
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        first_level_roots(&source.conn),
        vec!["Partition 0 (NTFS)".to_string()]
    );
    let parent = source
        .catalog_records()
        .into_iter()
        .find(|record| record.id == "windows")
        .and_then(|record| record.parent_id)
        .unwrap();
    assert_eq!(parent, placeholder);
}

#[test]
fn staging_partition_root_folds_self_referential_root() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-fold-self";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
    let placeholder = seed_placeholder(&source.conn, ds_id, 0, "Partition 0 (NTFS)");
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

    merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &single_done_manifest(ds_id, "Partition 0 (NTFS)"),
        &source.storage,
        None,
        None,
    )
    .unwrap();
    let parent = source
        .catalog_records()
        .into_iter()
        .find(|record| record.id == "docs")
        .and_then(|record| record.parent_id)
        .unwrap();
    assert_eq!(parent, placeholder);
}

#[test]
fn staging_partition_root_folds_slash_root_and_synthesizes_placeholder() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-fold-slash";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    insert_staging_row(&staging, ds_id, "root", None, "/", "directory");
    insert_staging_row(&staging, ds_id, "boot", Some("root"), "boot.ini", "file");
    drop(staging);

    merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &single_done_manifest(ds_id, "Partition 0 (FAT)"),
        &source.storage,
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        first_level_roots(&source.conn),
        vec!["Partition 0 (FAT)".to_string()]
    );
    let root_count = source
        .catalog_records()
        .iter()
        .filter(|record| matches!(record.name.as_str(), "/" | "\\" | "."))
        .count();
    assert_eq!(root_count, 0);
}

#[test]
fn staging_partition_root_keeps_real_fat_top_level_entries() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ds_id = "ds-fat-efi";
    let source = NativeSourceFixture::new(tmp.path(), ds_id);
    let placeholder = seed_placeholder(&source.conn, ds_id, 0, "Partition 0 (FAT)");
    let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
    insert_staging_row(&staging, ds_id, "efi", None, "EFI", "directory");
    insert_staging_row(&staging, ds_id, "boot", Some("efi"), "Boot", "directory");
    drop(staging);

    merge_all_staging_to_main_with_catalog(
        &source.conn,
        tmp.path(),
        ds_id,
        &single_done_manifest(ds_id, "Partition 0 (FAT)"),
        &source.storage,
        None,
        None,
    )
    .unwrap();
    let records = source.catalog_records();
    let efi = records.iter().find(|record| record.id == "efi").unwrap();
    let efi_parent = efi.parent_id.as_deref().unwrap();
    assert_eq!(efi_parent, placeholder);
    let efi_count = records.iter().filter(|record| record.id == "efi").count();
    assert_eq!(efi_count, 1);
}
