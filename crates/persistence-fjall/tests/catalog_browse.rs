use persistence_fjall::{
    instrumentation::ProcessMeasurement, CatalogBrowseQuery, CatalogEntryType, CatalogRecordStore,
    CatalogSort, DatabaseConfig, FileEntryRecord, PersistMode,
};

fn record(index: usize, name: &str) -> FileEntryRecord {
    FileEntryRecord {
        id: format!("entry-{index}"),
        parent_id: Some("parent".into()),
        data_source_id: "source".into(),
        path: format!("/{name}"),
        name: name.into(),
        entry_type: CatalogEntryType::Directory,
        size: Some((index % 3) as u64),
        ext: Some(format!("type{}", index % 3)),
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: None,
        modified: match index % 3 {
            0 => None,
            value => chrono::DateTime::from_timestamp(value as i64 - 2, 123),
        },
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: Some(0),
        unix_mode: None,
    }
}

fn query() -> CatalogBrowseQuery<'static> {
    CatalogBrowseQuery {
        source: "source",
        parent: Some("parent"),
        directories_only: true,
        show_hidden: false,
        sort: CatalogSort::Name,
        descending: false,
        offset: 0,
        limit: 20,
    }
}

#[test]
fn native_browse_preserves_natural_order_and_descending_value_ties() {
    let root = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let names = [
        "!", "0", "00", "1", "01", "2", "10", "A", "a", "a0", "a1", "a2", "a02", "a10", "b", "中",
    ];
    let records = names
        .iter()
        .enumerate()
        .map(|(index, name)| record(index, name))
        .collect::<Vec<_>>();
    store.upsert_batch(&records, PersistMode::SyncAll).unwrap();
    let snapshot = store.read_snapshot();
    let page = store.browse(&snapshot, &query()).unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.name.as_str())
            .collect::<Vec<_>>(),
        names
    );
    for sort in [
        CatalogSort::Size,
        CatalogSort::Modified,
        CatalogSort::Extension,
    ] {
        let mut request = query();
        request.directories_only = false;
        request.sort = sort;
        request.descending = true;
        let mut expected = records.clone();
        expected.sort_by_key(|record| std::cmp::Reverse(record.size.unwrap()));
        let actual = store.browse(&snapshot, &request).unwrap();
        assert_eq!(actual.records, expected);
        request.offset = 3;
        request.limit = 4;
        assert_eq!(
            store.browse(&snapshot, &request).unwrap().records,
            expected[3..7]
        );
    }
}

#[test]
fn browse_pages_and_fully_filtered_counts_do_not_scan_the_directory() {
    let root = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let mut batch = Vec::new();
    for index in 0..20_000 {
        let mut entry = record(index, &format!("folder{index}"));
        entry.system = index % 2 == 0;
        batch.push(entry);
        if batch.len() == 500 {
            store.upsert_batch(&batch, PersistMode::Buffer).unwrap();
            batch.clear();
        }
    }
    let snapshot = store.read_snapshot();
    let measurement = ProcessMeasurement::start().unwrap();
    let mut request = query();
    request.offset = 10;
    request.limit = 5;
    let page = store.browse(&snapshot, &request).unwrap();
    assert_eq!(page.total, 10_000);
    assert_eq!(page.records[0].name, "folder21");
    assert!(measurement.snapshot().scan_entries <= 60);
    assert_eq!(
        store
            .child_directory_count(&snapshot, "source", Some("parent"), false)
            .unwrap(),
        10_000
    );
    drop(measurement);
    let mut hidden = record(20_001, "hidden");
    hidden.parent_id = Some("all-hidden".into());
    hidden.hidden = true;
    store.upsert(&hidden, PersistMode::SyncAll).unwrap();
    let snapshot = store.read_snapshot();
    let measurement = ProcessMeasurement::start().unwrap();
    assert_eq!(
        store
            .child_directory_count(&snapshot, "source", Some("all-hidden"), false)
            .unwrap(),
        0
    );
    assert_eq!(measurement.snapshot().scan_entries, 0);
}

#[test]
fn parallel_replacement_moves_and_deletes_keep_parent_counts_consistent() {
    let root = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let entries = (0..1_024)
        .map(|index| record(index, &format!("folder{index}")))
        .collect::<Vec<_>>();
    for _ in 0..2 {
        let mut chunks = entries.chunks(128);
        store
            .upsert_parallel("source", &std::sync::atomic::AtomicBool::new(false), || {
                Ok(chunks.next().map(|batch| batch.to_vec()))
            })
            .unwrap();
        assert_eq!(
            store
                .browse(&store.read_snapshot(), &query())
                .unwrap()
                .total,
            1_024
        );
    }
    let mut moved = entries[0].clone();
    moved.parent_id = Some("other-parent".into());
    moved.system = true;
    store.upsert(&moved, PersistMode::SyncAll).unwrap();
    store
        .delete("source", &entries[1].id, PersistMode::SyncAll)
        .unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(store.browse(&snapshot, &query()).unwrap().total, 1_022);
    assert_eq!(
        store
            .child_directory_count(&snapshot, "source", Some("other-parent"), false)
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .child_directory_count(&snapshot, "source", Some("other-parent"), true)
            .unwrap(),
        1
    );
}

#[test]
fn streamed_replacement_counts_commit_and_rollback_with_catalog_records() {
    let root = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let mut parent = record(0, "parent");
    parent.id = "parent".into();
    parent.parent_id = None;
    let mut first = Some(vec![record(1, "first"), record(2, "second")]);
    store
        .replace_subtree_streamed::<std::io::Error, _>(
            "source",
            &parent,
            0,
            PersistMode::SyncAll,
            || Ok(first.take()),
        )
        .unwrap();
    let before = store
        .browse(&store.read_snapshot(), &query())
        .unwrap()
        .records;
    let mut pending = Some(vec![record(3, "third")]);
    let interrupted = store.replace_subtree_streamed::<std::io::Error, _>(
        "source",
        &parent,
        0,
        PersistMode::SyncAll,
        || {
            if let Some(records) = pending.take() {
                return Ok(Some(records));
            }
            std::fs::File::open(root.path().join("missing-evidence-input"))?;
            Ok(None)
        },
    );
    assert!(interrupted.is_err());
    let page = store.browse(&store.read_snapshot(), &query()).unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.records, before);
    let mut replacement = Some(vec![record(4, "fourth")]);
    store
        .replace_subtree_streamed::<std::io::Error, _>(
            "source",
            &parent,
            0,
            PersistMode::SyncAll,
            || Ok(replacement.take()),
        )
        .unwrap();
    let page = store.browse(&store.read_snapshot(), &query()).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.records[0].name, "fourth");
}
