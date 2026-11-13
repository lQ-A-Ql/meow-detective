use app_services::{artifact_service, case_service, mcp_host_service, source_db, timeline_service};
use chrono::{Duration, TimeZone, Utc};
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::fjall::{ArtifactRecord, PersistMode, TimelineRecord};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use rusqlite::Connection;
use std::{collections::BTreeMap, path::Path};
use transport::{paging::MAX_PAGE_OFFSET, CommandError, ErrorCategory};

#[path = "support/fjall_projection.rs"]
mod projection_support;

const SOURCE: &str = "offset-source";
const RECORDS: u64 = MAX_PAGE_OFFSET + 4;

fn register_source(case: &app_services::active_case::ActiveCase) {
    let source = DataSource {
        id: DataSourceId(SOURCE.to_owned()),
        name: SOURCE.to_owned(),
        kind: DataSourceKind::Raw,
        source_path: case.case_root.join("offset.raw"),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    case.with_conn(|connection| {
        let mut storage = DataSourceStorage::source_db(SOURCE, Some("windows"), None);
        storage.import_state = "ready".to_owned();
        DataSourceRepo::new(connection).insert_with_storage(&case.meta.id, &source, &storage)
    })
    .unwrap();
    let connection = source_db::open_source_db(&case.case_root, &source.id).unwrap();
    DataSourceRepo::new(&connection)
        .upsert_source_local_metadata(&case.meta.id, &source)
        .unwrap();
    let handle =
        source_db::open_registered_fjall_source_read_write(&case.case_root, &source.id).unwrap();
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    for start in (0..RECORDS).step_by(500) {
        let indexes = start..(start + 500).min(RECORDS);
        let artifacts = indexes
            .clone()
            .map(|index| ArtifactRecord {
                id: format!("artifact-{index:05}"),
                data_source_id: SOURCE.to_owned(),
                artifact_type: "EventLog".to_owned(),
                source_object_id: None,
                content_sha256: None,
                attrs: serde_json::json!({
                    "title":format!("event-{index:05}"),
                    "summary":"",
                    "createdAt":(base + Duration::seconds(index as i64)).to_rfc3339(),
                    "attrs":{}
                }),
            })
            .collect::<Vec<_>>();
        handle
            .artifact_upsert_batch(&artifacts, PersistMode::Buffer)
            .unwrap();
        let events = indexes
            .map(|index| TimelineRecord {
                id: format!("event-{index:05}"),
                data_source_id: SOURCE.to_owned(),
                source_object_id: format!("file-{index}"),
                event_type: "FILE_CREATED".to_owned(),
                timestamp: base + Duration::seconds(index as i64),
                title: format!("event-{index:05}"),
                description: String::new(),
                parser_id: None,
                parser_version: None,
                confidence: None,
                source_attribution: None,
                attrs: BTreeMap::new(),
            })
            .collect::<Vec<_>>();
        handle
            .timeline_upsert_batch(&events, PersistMode::Buffer)
            .unwrap();
    }
    handle.persist(PersistMode::SyncAll).unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        handle.source_store(),
        SOURCE,
        "offset-fixture",
        &"a".repeat(64),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    projection_support::mark_source_projection_complete(&handle, SOURCE);
}

fn artifact_cursor_page(
    connection: &Connection,
    root: &Path,
    case_id: &CaseId,
    offset: u64,
) -> Vec<String> {
    let mut remaining = offset;
    let mut cursor = None;
    while remaining > 0 {
        let limit = remaining.min(500) as u32;
        let page = artifact_service::get_artifact_rows_page_with_cursor_for_case(
            connection,
            root,
            case_id,
            None,
            0,
            limit,
            cursor.as_deref(),
        )
        .unwrap();
        assert_eq!(page.items.len(), limit as usize);
        remaining -= u64::from(limit);
        cursor = page.next_cursor;
    }
    artifact_service::get_artifact_rows_page_with_cursor_for_case(
        connection,
        root,
        case_id,
        None,
        0,
        3,
        cursor.as_deref(),
    )
    .unwrap()
    .items
    .into_iter()
    .map(|item| item.id)
    .collect()
}

fn timeline_cursor_page(
    connection: &Connection,
    root: &Path,
    case_id: &CaseId,
    offset: u64,
) -> Vec<String> {
    let mut remaining = offset;
    let mut cursor = None;
    while remaining > 0 {
        let limit = remaining.min(500) as u32;
        let mut query = timeline_service::TimelineQuery::unfiltered(0, limit);
        query.cursor = cursor.as_deref();
        let page =
            timeline_service::query_timeline_filtered_for_case(connection, root, case_id, query)
                .unwrap();
        assert_eq!(page.items.len(), limit as usize);
        remaining -= u64::from(limit);
        cursor = page.next_cursor;
    }
    let mut query = timeline_service::TimelineQuery::unfiltered(0, 3);
    query.cursor = cursor.as_deref();
    timeline_service::query_timeline_filtered_for_case(connection, root, case_id, query)
        .unwrap()
        .items
        .into_iter()
        .map(|item| item.id)
        .collect()
}

#[test]
fn maximum_offsets_match_cursor_pages_from_real_storage() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "offset-boundary", None).unwrap();
    register_source(&case);
    case.with_conn(|connection| {
        for offset in [9_999, MAX_PAGE_OFFSET] {
            let artifacts = artifact_service::get_artifact_rows_page_for_case(
                connection,
                &case.case_root,
                &case.meta.id,
                None,
                offset,
                3,
            )
            .unwrap();
            assert_eq!(artifacts.total, RECORDS);
            let artifact_ids = artifacts
                .items
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<_>>();
            assert_eq!(
                artifact_ids,
                artifact_cursor_page(connection, &case.case_root, &case.meta.id, offset)
            );
            let events = timeline_service::query_timeline_for_case(
                connection,
                &case.case_root,
                &case.meta.id,
                offset,
                3,
            )
            .unwrap();
            assert_eq!(events.total, RECORDS);
            let event_ids = events
                .items
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<_>>();
            assert_eq!(
                event_ids,
                timeline_cursor_page(connection, &case.case_root, &case.meta.id, offset)
            );
            for (position, (artifact, event)) in artifact_ids.iter().zip(event_ids).enumerate() {
                let index = RECORDS - offset - 1 - position as u64;
                assert_eq!(artifact, &format!("ds:{SOURCE}:artifact-{index:05}"));
                assert_eq!(event, format!("ds:{SOURCE}:event-{index:05}"));
            }
        }
        Ok(())
    })
    .unwrap();
}

fn assert_input_error(error: impl std::fmt::Display + transport::ServiceErrorCategory) {
    let error = CommandError::from_typed_service_error(error);
    assert_eq!(error.code, "INVALID_INPUT");
    assert_eq!(error.category, ErrorCategory::Validation.as_str());
    assert!(error.message.contains("nextCursor"));
}

#[test]
fn excessive_offsets_are_rejected_before_accessing_storage() {
    let connection = Connection::open_in_memory().unwrap();
    let root = Path::new("missing-case-storage");
    let case_id = CaseId("case-offset".to_owned());
    for offset in [MAX_PAGE_OFFSET + 1, u64::MAX] {
        for limit in [0, 1] {
            assert_input_error(
                artifact_service::get_artifact_rows_page_for_case(
                    &connection,
                    root,
                    &case_id,
                    None,
                    offset,
                    limit,
                )
                .unwrap_err(),
            );
            assert_input_error(
                artifact_service::get_artifact_rows_page_with_cursor_for_case(
                    &connection,
                    root,
                    &case_id,
                    None,
                    offset,
                    limit,
                    None,
                )
                .unwrap_err(),
            );
            assert_input_error(
                timeline_service::query_timeline(&connection, offset, limit).unwrap_err(),
            );
            assert_input_error(
                timeline_service::query_timeline_aggregated(&connection, offset, limit)
                    .unwrap_err(),
            );
            assert_input_error(
                timeline_service::query_timeline_for_case(
                    &connection,
                    root,
                    &case_id,
                    offset,
                    limit,
                )
                .unwrap_err(),
            );
        }
    }
}

#[test]
fn mcp_timeline_schema_and_execution_enforce_the_item_offset_limit() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "mcp-offset-boundary", None).unwrap();
    let tool = mcp_host_service::tool_catalog()
        .into_iter()
        .find(|tool| tool.name == "forensics.get_timeline_events")
        .unwrap();
    assert_eq!(
        tool.input_schema["properties"]["offset"]["maximum"],
        MAX_PAGE_OFFSET
    );
    assert!(tool.description.contains("nextCursor"));
    let connection = case.connection().unwrap();
    let context = || mcp_host_service::McpHostQueryContext {
        connection: &connection,
        case_root: &case.case_root,
        case_meta: &case.meta,
    };
    let page = mcp_host_service::execute_tool(
        context(),
        "forensics.get_timeline_events",
        &serde_json::json!({"offset":MAX_PAGE_OFFSET,"limit":1}),
    )
    .unwrap();
    assert_eq!(page["total"], 0);
    let error = mcp_host_service::execute_tool(
        context(),
        "forensics.get_timeline_events",
        &serde_json::json!({"offset":MAX_PAGE_OFFSET + 1,"limit":1}),
    )
    .unwrap_err();
    assert_input_error(error);
}
