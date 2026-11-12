use std::sync::Arc;

use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_mount::{MountNode, MountPath};

use super::{DirectorySnapshot, DirectorySnapshotCache};

fn entry(path: &str) -> Arc<FileEntry> {
    Arc::new(FileEntry {
        id: FileEntryId(format!("id:{path}")),
        parent_id: Some(FileEntryId("root".to_string())),
        data_source_id: DataSourceId("source".to_string()),
        path: path.to_string(),
        name: path.trim_start_matches('/').to_string(),
        entry_type: EntryType::File,
        size: Some(4),
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
    })
}

fn node(path: &MountPath, entry: &FileEntry) -> MountNode {
    MountNode {
        path: path.clone(),
        name: entry.name.clone(),
        is_dir: false,
        size: entry.size.unwrap_or(0),
        read_only: true,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        source_file_id: Some(entry.id.0.clone()),
    }
}

#[test]
fn snapshot_pages_and_resolves_catalog_entries_without_database_access() {
    let root = MountPath::root();
    let first_path = MountPath::parse("/first").expect("first path");
    let second_path = MountPath::parse("/second").expect("second path");
    let first = entry("first");
    let second = entry("second");
    let snapshot = Arc::new(DirectorySnapshot::new(vec![
        (node(&first_path, &first), Arc::clone(&first)),
        (node(&second_path, &second), Arc::clone(&second)),
    ]));
    let mut cache = DirectorySnapshotCache::new(snapshot.weight);

    cache.insert(root.clone(), Arc::clone(&snapshot));

    assert_eq!(
        cache.get(&root).expect("snapshot").page(1, 1)[0].name,
        "second"
    );
    assert_eq!(
        cache.find_entry(&first_path).expect("catalog entry").id.0,
        first.id.0
    );
}

#[test]
fn oversized_snapshot_is_returned_without_being_retained() {
    let root = MountPath::root();
    let path = MountPath::parse("/first").expect("first path");
    let entry = entry("first");
    let snapshot = Arc::new(DirectorySnapshot::new(vec![(node(&path, &entry), entry)]));
    let mut cache = DirectorySnapshotCache::new(snapshot.weight.saturating_sub(1));

    let returned = cache.insert(root.clone(), Arc::clone(&snapshot));

    assert!(Arc::ptr_eq(&returned, &snapshot));
    assert!(cache.get(&root).is_none());
    assert!(cache.find_entry(&path).is_none());
}

#[test]
fn evicting_a_snapshot_removes_its_global_path_entries() {
    let root = MountPath::root();
    let nested = MountPath::parse("/nested").expect("nested path");
    let first_path = MountPath::parse("/first").expect("first path");
    let second_path = MountPath::parse("/nested/second").expect("second path");
    let first = entry("first");
    let second = entry("nested/second");
    let first_snapshot = Arc::new(DirectorySnapshot::new(vec![(
        node(&first_path, &first),
        first,
    )]));
    let second_snapshot = Arc::new(DirectorySnapshot::new(vec![(
        node(&second_path, &second),
        second,
    )]));
    let mut cache = DirectorySnapshotCache::new(first_snapshot.weight.max(second_snapshot.weight));

    cache.insert(root.clone(), first_snapshot);
    assert!(cache.find_entry(&first_path).is_some());

    cache.insert(nested, second_snapshot);

    assert!(cache.get(&root).is_none());
    assert!(cache.find_entry(&first_path).is_none());
    assert!(cache.find_entry(&second_path).is_some());
}
