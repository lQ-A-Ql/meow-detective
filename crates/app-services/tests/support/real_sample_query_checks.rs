use app_services::artifact_service::{
    get_artifact_families_for_case as family_names,
    get_artifact_family_counts_for_case as family_counts,
    get_artifact_rows_page_with_cursor_for_case as artifact_page,
};
use app_services::file_service::{
    get_file_rows_for_case as file_rows, get_file_tree_for_case as file_roots,
    read_file_range_for_source_case as file_range,
};
use app_services::timeline_service::{
    get_timeline_facets_for_case as query_facets,
    query_timeline_filtered_for_case as timeline_page, TimelineQuery,
};
use app_services::{search_service, source_db};
use chrono::{TimeZone, Utc};
use domain::{CaseId, DataSourceId, FileEntryId};
use persistence_source::fjall::{
    instrumentation::ProcessMeasurement, ArtifactRecordStore, CatalogRecordStore,
    TimelineRecordStore,
};
use persistence_sqlite::repositories::job_repo::JobRepo;
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path, time::Instant};
use transport::commands::{
    FileSortDirectionDto, GetFileRowsRequest, SearchEntryTypeDto, SearchFilesRequest,
    SearchSortKeyDto,
};
use transport::dto::ViewerRangeRequestDto;

type State = (String, u64, u64, u64, u64, u64, u64, u64, String);

pub fn run(connection: &Connection, root: &Path, case: &CaseId, source: &DataSourceId) -> Value {
    let before_jobs = active_jobs(connection);
    if !before_jobs.is_empty() {
        return json!({"allRequiredPassed": false, "structuralMeasurementEligible": false,
            "status": "backgroundJobsActive", "activeJobs": before_jobs});
    }
    let before = source_states(connection, root, case);
    assert!(before.iter().any(|state| state.0 == source.0));
    let mut measurements = Vec::new();
    let browse = browse(connection, root, case, source, &mut measurements);
    let timeline = timeline(connection, root, case, &before, &mut measurements);
    let artifacts = artifacts(connection, root, case, &before, &mut measurements);
    let search = search(connection, root, case, &before, &mut measurements);
    let header = header(connection, root, case, source, &mut measurements);
    let after = source_states(connection, root, case);
    let remaining_jobs = active_jobs(connection);
    let stable = before == after && remaining_jobs.is_empty();
    let header_passed = header["status"] == "verified" || header["status"] == "empty";
    json!({"allRequiredPassed": stable && header_passed,
        "structuralMeasurementEligible": stable, "sourceStateUnchanged": before == after,
        "measurementScope": "one production API call; oracle checks excluded",
        "storageStateColumns": ["sourceId", "catalogRows", "fileRows", "timelineRows", "artifactRows",
            "catalogRevision", "timelineRevision", "artifactRevision", "generation"],
        "activeJobsAfter": remaining_jobs, "storageCountsAndRevisions": before,
        "browse": browse, "timeline": timeline, "artifacts": artifacts,
        "search": search, "header": header, "measurements": measurements})
}

fn active_jobs(connection: &Connection) -> Vec<String> {
    JobRepo::new(connection)
        .list_recent(1)
        .unwrap()
        .into_iter()
        .filter(|job| matches!(job.status.as_str(), "pending" | "running" | "cancelling"))
        .map(|job| job.id.0)
        .collect()
}

fn source_states(connection: &Connection, root: &Path, case: &CaseId) -> Vec<State> {
    let mut states = Vec::new();
    for (source, _) in source_db::ready_data_sources(connection, case).unwrap() {
        let handle = source_db::open_registered_fjall_source_read_only(root, &source.id).unwrap();
        let snapshot = handle.snapshot();
        handle
            .require_projection_complete(&snapshot, &source.id.0)
            .unwrap();
        let store = handle.source_store().clone();
        let catalog = CatalogRecordStore::from_source_store(store.clone());
        let timeline = TimelineRecordStore::from_source_store(store.clone());
        let artifacts = ArtifactRecordStore::from_source_store(store);
        states.push((
            source.id.0.clone(),
            catalog.count(&snapshot, Some(&source.id.0)).unwrap(),
            catalog.file_count(&snapshot, &source.id.0).unwrap(),
            timeline
                .count_by_time(&snapshot, &source.id.0, None, None)
                .unwrap(),
            artifacts.count(&snapshot, Some(&source.id.0)).unwrap(),
            catalog.cursor_revision(&snapshot).unwrap(),
            timeline.cursor_revision(&snapshot).unwrap(),
            artifacts.cursor_revision(&snapshot).unwrap(),
            handle
                .source_store()
                .database()
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ));
    }
    states.sort_by(|left, right| left.0.cmp(&right.0));
    states
}

fn measured<T>(name: &str, output: &mut Vec<Value>, operation: impl FnOnce() -> T) -> T {
    eprintln!("REAL_SAMPLE_QUERY_START {name}");
    let measurement = ProcessMeasurement::start().expect("isolated query measurement");
    let started = Instant::now();
    let result = operation();
    let elapsed = started.elapsed();
    let counters = measurement.snapshot();
    drop(measurement);
    let record = json!({"operation": name, "elapsedMicros": elapsed.as_micros(),
        "scanEntries": counters.scan_entries, "pointReads": counters.point_reads,
        "bytesRead": counters.bytes_read, "bytesWritten": counters.bytes_written,
        "sourceSessionOpens": counters.source_session_opens,
        "searchIndexOpens": counters.search_index_opens,
        "querySessionOpens": counters.query_session_opens,
        "rankedHits": counters.ranked_hits, "mergeComparisons": counters.merge_comparisons});
    eprintln!("REAL_SAMPLE_QUERY {record}");
    output.push(record);
    result
}

fn browse(
    connection: &Connection,
    root: &Path,
    case: &CaseId,
    source: &DataSourceId,
    measurements: &mut Vec<Value>,
) -> Value {
    let roots = measured("fileRoots", measurements, || {
        file_roots(connection, root, case, true).unwrap()
    });
    let selected = roots
        .iter()
        .filter(|node| node.data_source_id.as_deref() == Some(source.0.as_str()))
        .collect::<Vec<_>>();
    assert!(!selected.is_empty(), "imported source must expose a root");
    let Some(parent) = selected.iter().find(|node| node.has_children) else {
        return json!({"status": "empty", "roots": selected.len(), "listedRows": 0});
    };
    let mut request = GetFileRowsRequest {
        parent_id: Some(parent.id.clone()),
        limit: 50,
        show_hidden: true,
        ..Default::default()
    };
    let first = measured("fileRowsFirst", measurements, || {
        file_rows(connection, root, case, &request).unwrap()
    });
    assert!(first.rows.len() <= 50);
    let again = measured("fileRowsRepeat", measurements, || {
        file_rows(connection, root, case, &request).unwrap()
    });
    let ids = first.rows.iter().map(|row| &row.id).collect::<Vec<_>>();
    assert_eq!(
        ids,
        again.rows.iter().map(|row| &row.id).collect::<Vec<_>>()
    );
    let mut visited = first
        .rows
        .iter()
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(visited.len(), first.rows.len());
    if first.total_count > first.rows.len() as u64 {
        request.offset = first.rows.len() as u64;
        let second = measured("fileRowsNext", measurements, || {
            file_rows(connection, root, case, &request).unwrap()
        });
        assert_eq!(first.total_count, second.total_count);
        for row in second.rows {
            assert!(visited.insert(row.id));
        }
    }
    json!({"status": "verified", "roots": selected.len(), "directoryRows": first.total_count,
        "checkedRows": visited.len()})
}

fn timeline(
    connection: &Connection,
    root: &Path,
    case: &CaseId,
    states: &[State],
    measurements: &mut Vec<Value>,
) -> Value {
    let expected = states.iter().map(|state| state.3).sum::<u64>();
    let full = measured("timelineFacetsFull", measurements, || {
        query_facets(connection, root, case, None, None, None, 64).unwrap()
    });
    assert_eq!(full.total_events, expected);
    assert_eq!(
        full.event_types.iter().map(|kind| kind.count).sum::<u64>(),
        expected
    );
    assert_eq!(
        full.histogram
            .iter()
            .map(|bucket| bucket.count)
            .sum::<u64>(),
        expected
    );
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    let mut previous = None;
    for _ in 0..2 {
        let page = measured("timelineCursorPage", measurements, || {
            timeline_page(
                connection,
                root,
                case,
                TimelineQuery {
                    cursor: cursor.as_deref(),
                    ..TimelineQuery::unfiltered(0, 50)
                },
            )
            .unwrap()
        });
        assert_eq!(page.total, expected);
        for row in &page.items {
            let timestamp = chrono::DateTime::parse_from_rfc3339(&row.ts).unwrap();
            if let Some(previous) = previous {
                assert!(previous >= timestamp);
            }
            previous = Some(timestamp);
            assert!(seen.insert(row.id.clone()));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    if expected == 0 {
        assert!(full.histogram.is_empty());
        return json!({"status": "empty", "total": 0, "checkedRows": 0});
    }
    let from = chrono::DateTime::parse_from_rfc3339(full.start_ts.as_ref().unwrap())
        .unwrap()
        .timestamp();
    let until = chrono::DateTime::parse_from_rfc3339(full.end_ts.as_ref().unwrap())
        .unwrap()
        .timestamp();
    let narrow_from = from + (until - from) * 45 / 100;
    let narrow_until = from + (until - from) * 55 / 100;
    let begin = Utc.timestamp_opt(narrow_from, 0).unwrap();
    let end = Utc.timestamp_opt(narrow_until, 999_999_999).unwrap();
    let narrow = measured("timelineFacetsWindow", measurements, || {
        query_facets(
            connection,
            root,
            case,
            Some(&begin.to_rfc3339()),
            Some(&end.to_rfc3339()),
            None,
            64,
        )
        .unwrap()
    });
    let mut expected_narrow = 0;
    for state in states {
        let handle =
            source_db::open_registered_fjall_source_read_only(root, &DataSourceId(state.0.clone()))
                .unwrap();
        let store = TimelineRecordStore::from_source_store(handle.source_store().clone());
        expected_narrow += store
            .count_by_time(&handle.snapshot(), &state.0, Some(begin), Some(end))
            .unwrap();
    }
    assert_eq!(narrow.total_events, expected_narrow);
    assert_eq!(
        narrow
            .histogram
            .iter()
            .map(|bucket| bucket.count)
            .sum::<u64>(),
        expected_narrow
    );
    assert_eq!(narrow.event_types, full.event_types);
    json!({"status": "verified", "total": expected, "checkedRows": seen.len(), "narrowTotal": expected_narrow})
}

fn artifacts(
    connection: &Connection,
    root: &Path,
    case: &CaseId,
    states: &[State],
    measurements: &mut Vec<Value>,
) -> Value {
    let expected = states.iter().map(|state| state.4).sum::<u64>();
    let counts = measured("artifactFamilyCounts", measurements, || {
        family_counts(connection, root, case).unwrap()
    });
    let families = measured("artifactFamilies", measurements, || {
        family_names(connection, root, case).unwrap()
    });
    assert_eq!(
        counts.iter().map(|family| family.count).sum::<u64>(),
        expected
    );
    assert_eq!(
        families,
        counts
            .iter()
            .map(|family| family.family.clone())
            .collect::<Vec<_>>()
    );
    let first = measured("artifactRowsFirst", measurements, || {
        artifact_page(connection, root, case, None, 0, 50, None).unwrap()
    });
    assert_eq!(first.total, expected);
    let mut ids = first
        .items
        .iter()
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), first.items.len());
    if let Some(cursor) = first.next_cursor {
        let second = measured("artifactRowsNext", measurements, || {
            artifact_page(connection, root, case, None, 0, 50, Some(&cursor)).unwrap()
        });
        assert_eq!(second.total, expected);
        for row in second.items {
            assert!(ids.insert(row.id));
        }
    }
    json!({"status": if expected == 0 {"empty"} else {"verified"}, "total": expected,
        "families": counts, "checkedRows": ids.len()})
}

fn search(
    connection: &Connection,
    root: &Path,
    case: &CaseId,
    states: &[State],
    measurements: &mut Vec<Value>,
) -> Value {
    search_service::clear_case_file_search_cache(&case.0).unwrap();
    let mut request = SearchFilesRequest {
        query: String::new(),
        match_path: false,
        entry_type: SearchEntryTypeDto::Any,
        extensions: Vec::new(),
        data_source_ids: Vec::new(),
        sort_key: SearchSortKeyDto::Name,
        sort_direction: FileSortDirectionDto::Asc,
        offset: 0,
        limit: 50,
        cursor: None,
    };
    let first = measured("searchCold", measurements, || {
        search_service::search_files_for_case_cursor(connection, root, case, &request).unwrap()
    });
    assert!(
        first.coverage.complete,
        "real source search coverage must be complete"
    );
    assert_eq!(first.total, states.iter().map(|state| state.1).sum::<u64>());
    let first_ids = first
        .items
        .iter()
        .map(|row| row.file_id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(first_ids.len(), first.items.len());
    let mut checked = first_ids.len();
    if let Some(cursor) = &first.next_cursor {
        request.cursor = Some(cursor.clone());
        let second = measured("searchWarmContinuation", measurements, || {
            search_service::search_files_for_case_cursor(connection, root, case, &request).unwrap()
        });
        assert_eq!(first.total, second.total);
        assert!(second
            .items
            .iter()
            .all(|row| !first_ids.contains(&row.file_id)));
        checked += second.items.len();
        if states.len() <= 64 {
            let counters = measurements.last().unwrap();
            assert_eq!(counters["searchIndexOpens"], 0);
            assert_eq!(counters["querySessionOpens"], 0);
        }
    }
    json!({"status": if first.total == 0 {"empty"} else {"verified"}, "total": first.total,
        "checkedRows": checked, "coverage": first.coverage})
}

fn header(
    connection: &Connection,
    root: &Path,
    case: &CaseId,
    source: &DataSourceId,
    measurements: &mut Vec<Value>,
) -> Value {
    let handle = source_db::open_registered_fjall_source_read_only(root, source).unwrap();
    let snapshot = handle.snapshot();
    let catalog = CatalogRecordStore::from_source_store(handle.source_store().clone());
    if catalog.file_count(&snapshot, &source.0).unwrap() == 0 {
        return json!({"status": "empty", "reason": "source has no ordinary files"});
    }
    let mut after = None;
    let mut tried = 0;
    let mut failures = Vec::new();
    for _ in 0..32 {
        let page = catalog
            .list_files_by_path(
                &snapshot,
                &source.0,
                after
                    .as_ref()
                    .map(|(path, id): &(String, String)| (path.as_str(), id.as_str())),
                128,
            )
            .unwrap();
        for record in page.records {
            after = Some((record.path.clone(), record.id.clone()));
            if record.deleted
                || record.encrypted == Some(true)
                || record.size.unwrap_or(0) == 0
                || record.name.starts_with('$')
            {
                continue;
            }
            let global = source_db::GlobalFileId::new(source.clone(), FileEntryId(record.id))
                .encode()
                .0;
            tried += 1;
            let request = ViewerRangeRequestDto {
                handle_id: format!("file:{global}"),
                offset: 0,
                length: record.size.unwrap().min(4096) as u32,
            };
            match measured("fileHeaderRead", measurements, || file_range(connection, root, case, &request)) {
                Ok(first) => {
                    let bytes = first.raw_bytes.expect("hex preview bytes");
                    assert_eq!(bytes.len(), request.length as usize);
                    let repeated = measured("fileHeaderRepeat", measurements, || file_range(connection, root, case, &request).unwrap());
                    assert_eq!(repeated.raw_bytes.as_ref(), Some(&bytes));
                    return json!({"status": "verified", "fileId": global, "bytes": bytes.len(),
                        "sha256": hex::encode(Sha256::digest(&bytes)), "candidatesTried": tried,
                        "unreadableCandidates": failures});
                }
                Err(error) => failures.push(json!({"fileId": global,
                    "category": transport::ServiceErrorCategory::category(&error), "error": error.to_string()})),
            }
            if tried >= 32 {
                return json!({"status": "unreadable", "candidatesTried": tried, "failures": failures});
            }
        }
        if page.next_cursor.is_none() {
            break;
        }
    }
    json!({"status": "notVerified", "candidatesTried": tried, "failures": failures,
        "reason": "no readable nonempty ordinary file among bounded candidates"})
}
