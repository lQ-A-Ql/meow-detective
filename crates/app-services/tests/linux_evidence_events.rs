use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

use app_services::{case_service, cluster_service, source_db};
use chrono::{Duration, TimeZone, Utc};
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::fjall::{PersistMode, TimelineRecord};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    linux_import_set_repo::{LinuxImportSetMemberRecord, LinuxImportSetRecord, LinuxImportSetRepo},
};

#[path = "support/fjall_projection.rs"]
mod projection_support;
#[path = "support/timeline_projection.rs"]
mod timeline_projection;

fn register_set(conn: &rusqlite::Connection, case_id: &CaseId, set: &str) {
    LinuxImportSetRepo::new(conn)
        .insert(&LinuxImportSetRecord {
            id: set.into(),
            case_id: case_id.0.clone(),
            name: set.into(),
            root_path: "evidence".into(),
            import_state: "ready".into(),
            member_count: 2,
            ready_count: 2,
            failed_count: 0,
            last_error: None,
        })
        .unwrap();
}

fn publish_source(
    case: &app_services::active_case::ActiveCase,
    conn: &rusqlite::Connection,
    set: &str,
    index: u32,
    records: &[TimelineRecord],
) {
    let source_id = format!("{set}-{index}");
    let source = DataSource {
        id: DataSourceId(source_id.clone()),
        name: source_id.clone(),
        kind: DataSourceKind::Raw,
        source_path: case.case_root.join(format!("{source_id}.raw")),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source_id, Some("linux"), None);
    storage.import_state = "ready".into();
    DataSourceRepo::new(conn)
        .insert_with_storage(&case.meta.id, &source, &storage)
        .unwrap();
    LinuxImportSetRepo::new(conn)
        .insert_member(&LinuxImportSetMemberRecord {
            import_set_id: set.into(),
            member_index: index,
            source_path: source.source_path.display().to_string(),
            source_kind: "raw".into(),
            data_source_id: Some(source_id.clone()),
            import_state: "ready".into(),
            last_error: None,
        })
        .unwrap();
    let source_conn = source_db::open_source_db(&case.case_root, &source.id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&case.meta.id, &source)
        .unwrap();
    let native = source_db::open_fjall_source_for_build(&source_conn, &source.id).unwrap();
    native
        .timeline_upsert_batch(records, PersistMode::SyncAll)
        .unwrap();
    timeline_projection::mark_complete(native.source_store(), &source_id, records.len() as u64);
    let receipt = projection_support::mark_source_projection_complete(&native, &source_id);
    persistence_source::fjall::seal_source_generation_with_control(
        native.source_store(),
        &source_id,
        "events-fixture",
        &hex::encode(receipt.batch_digest),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
}

fn records(set: &str, source: u32, count: usize) -> Vec<TimelineRecord> {
    let start = Utc.with_ymd_and_hms(2026, 11, 27, 0, 0, 0).unwrap();
    (0..count)
        .map(|index| TimelineRecord {
            id: format!("event-{index:05}"),
            data_source_id: format!("{set}-{source}"),
            source_object_id: format!("object-{index}"),
            event_type: "FILE_CREATED".into(),
            timestamp: start - Duration::microseconds(index as i64 * 2 + i64::from(source)),
            title: format!("event {index}"),
            description: "persisted event".into(),
            parser_id: Some("fixture.events".into()),
            parser_version: Some("1".into()),
            confidence: Some(1.0),
            source_attribution: None,
            attrs: BTreeMap::new(),
        })
        .collect()
}

#[test]
fn event_merge_reaches_beyond_two_thousand_records_and_keeps_set_and_case_ownership() {
    let root = tempfile::tempdir().unwrap();
    let case = case_service::create_case(root.path(), "Event merge", None).unwrap();
    let conn = case.connection().unwrap();
    register_set(&conn, &case.meta.id, "set-a");
    register_set(&conn, &case.meta.id, "set-ab");
    let mut expected = Vec::new();
    for index in 0..2 {
        let events = records("set-a", index, 2300);
        expected.extend(events.clone());
        publish_source(&case, &conn, "set-a", index, &events);
        publish_source(&case, &conn, "set-ab", index, &records("set-ab", index, 3));
    }
    expected.sort_by(|left, right| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.data_source_id.cmp(&right.data_source_id))
    });
    for (offset, limit) in [(0, 7), (2001, 201), (4200, 300), (4595, 10), (4600, 8)] {
        let actual = cluster_service::get_linux_evidence_events(
            &conn,
            &case.case_root,
            &case.meta.id,
            "set-a",
            offset,
            limit,
        )
        .unwrap();
        let expected = expected
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|record| (record.data_source_id.clone(), record.id.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            actual
                .iter()
                .map(|record| (record.data_source_id.clone(), record.event_id.clone()))
                .collect::<Vec<_>>(),
            expected
        );
    }
    assert!(cluster_service::get_linux_evidence_events(
        &conn,
        &case.case_root,
        &CaseId("other-case".into()),
        "set-a",
        0,
        10,
    )
    .is_err());
    DataSourceRepo::new(&conn)
        .update_import_state(
            &DataSourceId("set-a-0".into()),
            "failed",
            Some("injected source failure"),
        )
        .unwrap();
    assert!(cluster_service::get_linux_evidence_events(
        &conn,
        &case.case_root,
        &case.meta.id,
        "set-a",
        0,
        10,
    )
    .is_err());
    DataSourceRepo::new(&conn)
        .update_import_state(&DataSourceId("set-a-0".into()), "ready", None)
        .unwrap();
    conn.execute(
        "UPDATE linux_import_set_members SET import_state='failed'
        WHERE import_set_id='set-a' AND member_index=1",
        [],
    )
    .unwrap();
    assert!(cluster_service::get_linux_evidence_events(
        &conn,
        &case.case_root,
        &case.meta.id,
        "set-a",
        0,
        10,
    )
    .is_err());
}
