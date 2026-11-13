use super::{
    config::RunConfig,
    metrics::{measure, ScenarioCounters},
};
use persistence_source::fjall::{
    DatabaseConfig, KeyspaceName, PersistMode, ScanBounds, SourceStore, WriteAction,
};

pub(super) fn run(config: &RunConfig) -> serde_json::Value {
    let root = tempfile::tempdir().unwrap();
    let source = SourceStore::open(root.path().join("source"), DatabaseConfig::default()).unwrap();
    let count = config.scale.clone_records;
    for start in (0..count).step_by(1024) {
        let batch = (start..(start + 1024).min(count))
            .map(|index| WriteAction::Put {
                keyspace: KeyspaceName::Meta,
                key: format!("benchmark-record-{index:09}").into_bytes(),
                value: vec![(index % 251) as u8; 128],
            })
            .collect::<Vec<_>>();
        source
            .database()
            .write_batch(&batch, PersistMode::Buffer)
            .unwrap();
    }
    source.database().persist(PersistMode::SyncAll).unwrap();
    let totals = source_totals(&source);
    assert_eq!(totals.payload_records, count as u64);
    let targets = (0..config.runs + config.warmups)
        .map(|index| root.path().join(format!("clone-{index}")))
        .collect::<Vec<_>>();
    let mut next = 0;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = measure(
        config,
        "reprocess_generation_clone",
        serde_json::json!({
            "payloadRecords": count,
            "payloadValueBytes": 128,
            "totalEntries": totals.entries,
            "totalLogicalBytes": totals.bytes,
            "populatedKeyspaces": totals.populated_keyspaces,
            "totalKeyspaces": totals.keyspaces,
        }),
        || {
            let mut progress = None;
            let target = source
                .clone_to_with_control(
                    &targets[next],
                    DatabaseConfig::default(),
                    &cancel,
                    |sample| {
                        progress = Some(sample);
                    },
                )
                .unwrap();
            next += 1;
            let sample_key = format!("benchmark-record-{:09}", count - 1);
            let value = target
                .database()
                .read_snapshot()
                .get(KeyspaceName::Meta, sample_key.as_bytes())
                .unwrap();
            assert_eq!(value.unwrap(), vec![((count - 1) % 251) as u8; 128]);
            let progress = progress.expect("clone completion progress");
            assert!(progress.durable);
            assert_eq!(progress.records_copied, totals.entries);
            assert_eq!(progress.bytes_copied, totals.bytes);
            assert_eq!(progress.completed_keyspaces, totals.keyspaces);
            assert_eq!(progress.total_keyspaces, totals.keyspaces);
            ScenarioCounters {
                returned_rows: Some(progress.records_copied),
                clone_records_copied: Some(progress.records_copied),
                clone_bytes_copied: Some(progress.bytes_copied),
                ..Default::default()
            }
        },
    );
    assert_eq!(next, targets.len());
    result
}

#[derive(Default)]
struct SourceTotals {
    payload_records: u64,
    entries: u64,
    bytes: u64,
    populated_keyspaces: u64,
    keyspaces: usize,
}

fn source_totals(source: &SourceStore) -> SourceTotals {
    let snapshot = source.database().read_snapshot();
    let mut totals = SourceTotals::default();
    for keyspace in source.database().keyspace_names() {
        totals.keyspaces += 1;
        let mut cursor = None;
        let mut populated = false;
        loop {
            let page = snapshot
                .range_keyset(keyspace, ScanBounds::unbounded(), cursor.as_deref(), 1024)
                .unwrap();
            if page.is_empty() {
                break;
            }
            populated = true;
            for entry in &page {
                totals.entries = totals.entries.checked_add(1).unwrap();
                totals.bytes = totals
                    .bytes
                    .checked_add((entry.key.len() + entry.value.len()) as u64)
                    .unwrap();
                if keyspace == KeyspaceName::Meta && entry.key.starts_with(b"benchmark-record-") {
                    assert_eq!(entry.value.len(), 128);
                    totals.payload_records += 1;
                }
            }
            cursor = page.last().map(|entry| entry.key.clone());
        }
        totals.populated_keyspaces += u64::from(populated);
    }
    totals
}
