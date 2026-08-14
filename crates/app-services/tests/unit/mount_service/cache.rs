use std::sync::Arc;

use chrono::Utc;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_mount::MountPath;

use super::MountMetadataCache;

fn entry(id: &str) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: None,
        data_source_id: DataSourceId("source".to_string()),
        path: id.to_string(),
        name: id.to_string(),
        entry_type: EntryType::File,
        size: Some(1),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        created_at: Some(Utc::now()),
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

#[test]
fn bounds_metadata_entries_and_refreshes_existing_paths() {
    let first = MountPath::parse("/first").expect("first path");
    let second = MountPath::parse("/second").expect("second path");
    let mut cache = MountMetadataCache::new(1);

    cache.insert(first.clone(), Arc::new(entry("first")));
    cache.insert(first.clone(), Arc::new(entry("replacement")));
    assert_eq!(cache.get(&first).expect("cached entry").id.0, "replacement");

    cache.insert(second.clone(), Arc::new(entry("second")));
    assert!(cache.get(&first).is_none());
    assert_eq!(cache.get(&second).expect("second entry").id.0, "second");
}
