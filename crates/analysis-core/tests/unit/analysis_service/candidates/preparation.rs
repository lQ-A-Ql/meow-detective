use super::*;
use crate::analysis_service::candidates::evidence_candidates_for_categories_with_preparation;
use persistence_source::fjall::{instrumentation, CatalogRecordStore, PersistMode};
use std::sync::atomic::AtomicBool;
crate::native_source_test_support!();

fn record(id: usize, path: &str) -> FileEntryRecord {
    FileEntryRecord {
        id: format!("record-{id:05}"),
        parent_id: None,
        data_source_id: "source".into(),
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        entry_type: CatalogEntryType::File,
        size: Some(32),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: None,
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: Some(17),
        unix_mode: None,
    }
}

#[test]
fn discovery_reuses_catalog_fields_and_preparation_never_requeries_storage() {
    let fixture = test_support::NativeSourceFixture::new("source", "case");
    let store = persistence_source::detail::open_store_for_write(&fixture)
        .unwrap()
        .unwrap();
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let mut records = (0..1_200)
        .map(|id| record(id, &format!("/ordinary/{id}.txt")))
        .collect::<Vec<_>>();
    records.push(record(1200, "/etc/localtime"));
    records.push(record(1201, "/etc/timezone"));
    let mut system = record(1202, "[P17]/Windows/System32/config/SYSTEM");
    system.encrypted = None;
    records.push(system);
    records.push(record(
        1203,
        "[P17]/Users/A/AppData/Local/Google/Chrome/User Data/Local State",
    ));
    catalog
        .upsert_batch(&records, PersistMode::SyncAll)
        .unwrap();
    instrumentation::enable();
    instrumentation::reset();
    let mut progress = Vec::new();
    let (candidates, _, preparation) = evidence_candidates_for_categories_with_preparation(
        &fixture,
        &["Registry", "BrowserHistory", "LinuxArtifacts"],
        &AtomicBool::new(false),
        |records| progress.push(records),
    )
    .unwrap();
    let discovered = instrumentation::snapshot();
    assert_eq!(discovered.point_reads, 0);
    assert!(discovered.scan_entries <= records.len() as u64 + 10);
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.file_id.0 == "record-01202")
        .unwrap();
    assert_eq!(candidate.partition_index, Some(17));
    assert!(
        candidate.encrypted,
        "unknown encryption must remain blocked"
    );
    assert_eq!(
        preparation
            .shortest_suffix("/etc/localtime")
            .unwrap()
            .file_id
            .0,
        "record-01200"
    );
    assert_eq!(
        preparation
            .suffix("source", "/windows/system32/config/system")
            .unwrap()
            .partition_index,
        Some(17)
    );
    assert!(preparation
        .suffix("another-source", "/windows/system32/config/system")
        .is_none());
    assert_eq!(preparation.matching("source", "/local state").len(), 1);
    assert_eq!(instrumentation::snapshot(), discovered);
    assert_eq!(progress, [512, 1024, 1204]);
    let cancel = AtomicBool::new(false);
    let result = evidence_candidates_for_categories_with_preparation(
        &fixture,
        &["LinuxArtifacts"],
        &cancel,
        |records| {
            if records >= 512 {
                cancel.store(true, std::sync::atomic::Ordering::Release);
            }
        },
    );
    assert!(matches!(result, Err(AnalysisServiceError::Cancelled)));
}
