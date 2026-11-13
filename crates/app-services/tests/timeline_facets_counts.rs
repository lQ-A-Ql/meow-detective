use app_services::active_case::ActiveCase;
use app_services::timeline_service::get_timeline_facets_for_case;
use chrono::{DateTime, TimeZone, Utc};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::fjall::{
    instrumentation, PersistMode, TimelineRecord, TimelineRecordStore,
};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::collections::BTreeMap;
use transport::dto::{TimelineFacetCountDto, TimelineFacetsDto};

const SOURCE: &str = "source-a";
const DAY_SECONDS: i64 = 86_400;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WriteMode {
    /// `upsert_batch` initializes and maintains the `timeline.count` counters.
    Counted,
    /// `upsert_batch_independent` skips count maintenance.
    Independent,
}

#[derive(Debug, Clone)]
struct FacetFilter {
    start: Option<String>,
    end: Option<String>,
    event_type: Option<&'static str>,
    buckets: u32,
}

fn base_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap()
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(seconds, 0).unwrap()
}

fn record(source: &str, id: &str, event_type: &str, when: DateTime<Utc>) -> TimelineRecord {
    TimelineRecord {
        id: id.to_owned(),
        data_source_id: source.to_owned(),
        source_object_id: format!("file-{id}"),
        event_type: event_type.to_owned(),
        timestamp: when,
        title: format!("title-{id}"),
        description: format!("description-{id}"),
        parser_id: Some("test.facets".to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(1.0),
        source_attribution: Some("fixture".to_owned()),
        attrs: Default::default(),
    }
}

fn register_ready_source(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &domain::CaseId,
    source_id: &str,
) -> persistence_sqlite::DbResult<rusqlite::Connection> {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::E01,
        source_path: case_root.join(format!("{source_id}.E01")),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some("linux"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(case_conn).insert_with_storage(case_id, &source, &storage)?;
    let source_conn = app_services::source_db::open_source_db(case_root, &source.id)?;
    DataSourceRepo::new(&source_conn).upsert_source_local_metadata(case_id, &source)?;
    mark_source_projection_complete(case_root, source_id)?;
    Ok(source_conn)
}

fn mark_source_projection_complete(
    case_root: &std::path::Path,
    source_id: &str,
) -> persistence_sqlite::DbResult<()> {
    let source = app_services::source_db::open_registered_fjall_source_read_write(
        case_root,
        &DataSourceId(source_id.to_string()),
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let key = persistence_source::fjall::encode_key(
        "projection",
        &[persistence_source::fjall::KeyPart::Text(
            source_id.to_string(),
        )],
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let marker = persistence_source::fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    source
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            PersistMode::SyncAll,
        )
        .map(|_| ())
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
}

fn publish(case_root: &std::path::Path, records: &[TimelineRecord], mode: WriteMode) {
    publish_source(case_root, SOURCE, records, mode);
}

fn publish_source(
    case_root: &std::path::Path,
    source_id: &str,
    records: &[TimelineRecord],
    mode: WriteMode,
) {
    let handle = app_services::source_db::open_registered_fjall_source_read_write(
        case_root,
        &DataSourceId(source_id.to_string()),
    )
    .unwrap();
    let store = TimelineRecordStore::from_source_store(handle.source_store().clone());
    match mode {
        WriteMode::Counted => store.upsert_batch(records, PersistMode::SyncAll).unwrap(),
        WriteMode::Independent => store
            .upsert_batch_independent(records, PersistMode::SyncAll)
            .unwrap(),
    }
    drop(store);
    drop(handle);
}

fn prepare_case(
    temp: &tempfile::TempDir,
    name: &str,
    records: &[TimelineRecord],
    mode: WriteMode,
) -> ActiveCase {
    let active =
        app_services::case_service::create_case(temp.path(), name, Some("tester")).unwrap();
    active
        .with_conn(|case_conn| {
            drop(register_ready_source(
                case_conn,
                &active.case_root,
                &active.meta.id,
                SOURCE,
            )?);
            Ok(())
        })
        .unwrap();
    publish(&active.case_root, records, mode);
    active
}

fn facets(active: &ActiveCase, filter: &FacetFilter) -> TimelineFacetsDto {
    active
        .with_conn(|case_conn| {
            get_timeline_facets_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                filter.start.as_deref(),
                filter.end.as_deref(),
                filter.event_type,
                filter.buckets,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap()
}

fn count_pairs(items: &[TimelineFacetCountDto]) -> Vec<(String, u64)> {
    items
        .iter()
        .map(|item| (item.value.clone(), item.count))
        .collect()
}

fn assert_same_facets(left: &TimelineFacetsDto, right: &TimelineFacetsDto, label: &str) {
    assert_eq!(left.total_events, right.total_events, "{label} total");
    assert_eq!(left.start_ts, right.start_ts, "{label} start");
    assert_eq!(left.end_ts, right.end_ts, "{label} end");
    assert_eq!(
        count_pairs(&left.event_types),
        count_pairs(&right.event_types),
        "{label} event types"
    );
    assert_eq!(
        count_pairs(&left.data_sources),
        count_pairs(&right.data_sources),
        "{label} data sources"
    );
    assert_eq!(
        left.histogram.len(),
        right.histogram.len(),
        "{label} bucket count"
    );
    for (index, (left, right)) in left.histogram.iter().zip(&right.histogram).enumerate() {
        assert_eq!(
            left.start_ts, right.start_ts,
            "{label} bucket {index} start"
        );
        assert_eq!(left.end_ts, right.end_ts, "{label} bucket {index} end");
        assert_eq!(left.count, right.count, "{label} bucket {index} count");
    }
}

/// 60 records over six days and three event types, aligned to hour boundaries.
fn comparison_records() -> Vec<TimelineRecord> {
    let types = ["FILE_CREATED", "FILE_MODIFIED", "LOGON"];
    let base = base_time().timestamp();
    let mut records = Vec::new();
    for day in 0..6 {
        for hour in 0..10 {
            let when = at(base + day * DAY_SECONDS + hour * 3_600);
            records.push(record(
                SOURCE,
                &format!("r{day:02}{hour:02}"),
                types[(day * 10 + hour) as usize % types.len()],
                when,
            ));
        }
    }
    records
}

fn comparison_filters() -> Vec<FacetFilter> {
    let midnight = |days: i64| at(base_time().timestamp() + days * DAY_SECONDS).to_rfc3339();
    vec![
        FacetFilter {
            start: None,
            end: None,
            event_type: None,
            buckets: 12,
        },
        FacetFilter {
            start: None,
            end: None,
            event_type: Some("FILE_CREATED"),
            buckets: 12,
        },
        FacetFilter {
            start: Some(midnight(2)),
            end: None,
            event_type: None,
            buckets: 12,
        },
        FacetFilter {
            start: None,
            end: Some(midnight(3)),
            event_type: None,
            buckets: 12,
        },
        FacetFilter {
            start: Some(midnight(1)),
            end: Some(midnight(4)),
            event_type: Some("LOGON"),
            buckets: 12,
        },
        FacetFilter {
            start: Some(midnight(1)),
            end: Some(midnight(1)),
            event_type: None,
            buckets: 12,
        },
        FacetFilter {
            start: Some(midnight(20)),
            end: Some(midnight(21)),
            event_type: None,
            buckets: 12,
        },
        FacetFilter {
            start: Some(midnight(1)),
            end: Some(midnight(1)),
            event_type: Some("FILE_MODIFIED"),
            buckets: 12,
        },
    ]
}

#[test]
fn timeline_facets_agree_between_counted_and_scanned_sources() {
    let records = comparison_records();
    let counted_temp = tempfile::TempDir::new().unwrap();
    let counted = prepare_case(&counted_temp, "facets-counts", &records, WriteMode::Counted);
    let scanned_temp = tempfile::TempDir::new().unwrap();
    let scanned = prepare_case(
        &scanned_temp,
        "facets-scan",
        &records,
        WriteMode::Independent,
    );
    for filter in comparison_filters() {
        let label = format!("{filter:?}");
        let counted_facets = facets(&counted, &filter);
        let scanned_facets = facets(&scanned, &filter);
        assert_same_facets(&counted_facets, &scanned_facets, &label);
        assert!(
            !counted_facets.event_types.is_empty(),
            "{label} keeps type totals"
        );
        let bucket_total = counted_facets
            .histogram
            .iter()
            .map(|bucket| bucket.count)
            .sum::<u64>();
        assert_eq!(bucket_total, counted_facets.total_events, "{label} buckets");
    }
    let unfiltered = facets(
        &counted,
        &FacetFilter {
            start: None,
            end: None,
            event_type: None,
            buckets: 12,
        },
    );
    assert_eq!(unfiltered.total_events, records.len() as u64);
    assert_eq!(unfiltered.data_sources.len(), 1);
    assert_eq!(unfiltered.data_sources[0].value, SOURCE);
    assert_eq!(unfiltered.event_types.len(), 3);
    let empty = facets(
        &counted,
        &FacetFilter {
            start: Some(at(base_time().timestamp() + 20 * DAY_SECONDS).to_rfc3339()),
            end: Some(at(base_time().timestamp() + 21 * DAY_SECONDS).to_rfc3339()),
            event_type: None,
            buckets: 12,
        },
    );
    assert_eq!(empty.total_events, 0);
    assert_eq!(empty.start_ts, None);
    assert_eq!(empty.end_ts, None);
    assert!(empty.histogram.is_empty());
    assert!(empty.data_sources.is_empty());
    assert_eq!(empty.event_types.len(), 3);
}

/// 1320 records over 20 days: six `FILE_CREATED` and sixty `BACKGROUND` per day.
fn spread_records() -> Vec<TimelineRecord> {
    let base = base_time().timestamp();
    let mut records = Vec::new();
    for day in 0..20 {
        let day_start = base + day * DAY_SECONDS;
        for slot in 0..6 {
            records.push(record(
                SOURCE,
                &format!("created-{day:02}-{slot}"),
                "FILE_CREATED",
                at(day_start + slot * 3_600),
            ));
        }
        for slot in 0..60 {
            records.push(record(
                SOURCE,
                &format!("background-{day:02}-{slot:02}"),
                "BACKGROUND",
                at(day_start + slot * 1_440),
            ));
        }
    }
    records
}

fn narrow_window() -> FacetFilter {
    let day = base_time().timestamp() + 10 * DAY_SECONDS;
    FacetFilter {
        start: Some(at(day).to_rfc3339()),
        end: Some(at(day + DAY_SECONDS - 1).to_rfc3339()),
        event_type: Some("FILE_CREATED"),
        buckets: 24,
    }
}

#[test]
fn timeline_facets_narrow_window_avoids_the_full_scan_when_counts_are_ready() {
    let records = spread_records();
    let total = records.len() as u64;
    let filter = narrow_window();

    let counted_temp = tempfile::TempDir::new().unwrap();
    let counted_case = prepare_case(
        &counted_temp,
        "facets-narrow-counts",
        &records,
        WriteMode::Counted,
    );
    instrumentation::enable();
    instrumentation::reset();
    let counted = facets(&counted_case, &filter);
    let counted_counters = instrumentation::snapshot();
    instrumentation::disable();
    assert_eq!(counted.total_events, 6);
    assert!(
        counted_counters.point_reads > 0 && counted_counters.point_reads <= 32,
        "narrow facets must not materialize out-of-window records: {counted_counters:?}"
    );
    assert!(
        counted_counters.scan_entries <= total / 4,
        "count-driven facets scanned {} entries of {total}: {counted_counters:?}",
        counted_counters.scan_entries
    );

    let scanned_temp = tempfile::TempDir::new().unwrap();
    let scanned_case = prepare_case(
        &scanned_temp,
        "facets-narrow-scan",
        &records,
        WriteMode::Independent,
    );
    instrumentation::enable();
    instrumentation::reset();
    let scanned = facets(&scanned_case, &filter);
    let scanned_counters = instrumentation::snapshot();
    instrumentation::disable();
    assert_eq!(scanned.total_events, 6);
    assert!(
        scanned_counters.scan_entries >= total,
        "uncounted facets must scan every record ({total}): {scanned_counters:?}"
    );
    assert_same_facets(&counted, &scanned, "narrow window");
}

#[test]
fn nonempty_facets_retain_sources_with_zero_matching_events() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = prepare_case(
        &temp,
        "facets-empty-source",
        &comparison_records(),
        WriteMode::Counted,
    );
    active
        .with_conn(|conn| {
            drop(register_ready_source(
                conn,
                &active.case_root,
                &active.meta.id,
                "source-b",
            )?);
            Ok(())
        })
        .unwrap();
    let result = facets(
        &active,
        &FacetFilter {
            start: None,
            end: None,
            event_type: None,
            buckets: 12,
        },
    );
    assert_eq!(
        count_pairs(&result.data_sources),
        vec![(SOURCE.to_owned(), 60), ("source-b".to_owned(), 0)]
    );
}

#[test]
fn uncounted_full_window_materializes_each_record_only_once() {
    let temp = tempfile::TempDir::new().unwrap();
    let records = spread_records();
    let active = prepare_case(
        &temp,
        "facets-single-scan",
        &records,
        WriteMode::Independent,
    );
    instrumentation::enable();
    instrumentation::reset();
    let result = facets(
        &active,
        &FacetFilter {
            start: None,
            end: None,
            event_type: None,
            buckets: 24,
        },
    );
    let counters = instrumentation::snapshot();
    instrumentation::disable();
    assert_eq!(result.total_events, records.len() as u64);
    assert_eq!(
        result
            .histogram
            .iter()
            .map(|bucket| bucket.count)
            .sum::<u64>(),
        result.total_events
    );
    assert!(
        counters.point_reads <= records.len() as u64 + 16,
        "duplicate record reads: {counters:?}"
    );
}

fn assert_record_oracle(
    result: &TimelineFacetsDto,
    records: &[TimelineRecord],
    filter: &FacetFilter,
) {
    let parse = |value: &str| DateTime::parse_from_rfc3339(value).unwrap().timestamp();
    let start = filter.start.as_deref().map(parse);
    let end = filter.end.as_deref().map(parse);
    let selected = records
        .iter()
        .filter(|record| {
            filter
                .event_type
                .is_none_or(|kind| record.event_type == kind)
                && start.is_none_or(|bound| record.timestamp.timestamp() >= bound)
                && end.is_none_or(|bound| record.timestamp.timestamp() <= bound)
        })
        .collect::<Vec<_>>();
    let mut types = BTreeMap::<String, u64>::new();
    for record in records {
        *types.entry(record.event_type.clone()).or_default() += 1;
    }
    assert_eq!(
        count_pairs(&result.event_types),
        types.into_iter().collect::<Vec<_>>()
    );
    assert_eq!(result.total_events, selected.len() as u64);
    let expected_first = selected
        .iter()
        .map(|record| record.timestamp.timestamp())
        .min();
    let expected_last = selected
        .iter()
        .map(|record| record.timestamp.timestamp())
        .max();
    assert_eq!(result.start_ts.as_deref().map(parse), expected_first);
    assert_eq!(result.end_ts.as_deref().map(parse), expected_last);
    if selected.is_empty() {
        assert!(result.data_sources.is_empty());
        assert!(result.histogram.is_empty());
        return;
    }
    let sources = [SOURCE, "source-b", "source-empty"].map(|source| {
        let count = selected
            .iter()
            .filter(|record| record.data_source_id == source)
            .count();
        (source.to_owned(), count as u64)
    });
    assert_eq!(count_pairs(&result.data_sources), sources);
    for bucket in &result.histogram {
        let first = parse(&bucket.start_ts);
        let last = parse(&bucket.end_ts);
        let expected = selected
            .iter()
            .filter(|record| (first..=last).contains(&record.timestamp.timestamp()))
            .count() as u64;
        assert_eq!(bucket.count, expected);
    }
    for pair in result.histogram.windows(2) {
        assert_eq!(parse(&pair[0].end_ts) + 1, parse(&pair[1].start_ts));
    }
    assert_eq!(
        result
            .histogram
            .iter()
            .map(|bucket| bucket.count)
            .sum::<u64>(),
        result.total_events
    );
}

#[test]
fn mixed_counted_sources_match_randomized_record_oracles() {
    let mut random = StdRng::seed_from_u64(0xfac_e750);
    let types = ["FILE_CREATED", "FILE_MODIFIED", "LOGON"];
    let records = [SOURCE, "source-b"]
        .into_iter()
        .flat_map(|source| {
            (0..160)
                .map(|index| {
                    let seconds = random.gen_range(-DAY_SECONDS..7 * DAY_SECONDS);
                    let micros = random.gen_range(0..1_000_000);
                    let kind = types[random.gen_range(0..types.len())];
                    record(
                        source,
                        &format!("{source}-{index}"),
                        kind,
                        at(base_time().timestamp() + seconds)
                            + chrono::Duration::microseconds(micros),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for (first_mode, second_mode) in [
        (WriteMode::Counted, WriteMode::Independent),
        (WriteMode::Independent, WriteMode::Counted),
    ] {
        let temporary = tempfile::TempDir::new().unwrap();
        let first = records
            .iter()
            .filter(|record| record.data_source_id == SOURCE)
            .cloned()
            .collect::<Vec<_>>();
        let second = records
            .iter()
            .filter(|record| record.data_source_id == "source-b")
            .cloned()
            .collect::<Vec<_>>();
        let active = prepare_case(&temporary, "facets-mixed-oracle", &first, first_mode);
        active
            .with_conn(|conn| {
                for source in ["source-b", "source-empty"] {
                    drop(register_ready_source(
                        conn,
                        &active.case_root,
                        &active.meta.id,
                        source,
                    )?);
                }
                Ok(())
            })
            .unwrap();
        publish_source(&active.case_root, "source-b", &second, second_mode);
        for iteration in 0..40 {
            let start =
                base_time().timestamp() + random.gen_range(-2 * DAY_SECONDS..8 * DAY_SECONDS);
            let end = start + random.gen_range(0..3 * DAY_SECONDS);
            let filter = FacetFilter {
                start: (iteration % 3 != 0).then(|| at(start).to_rfc3339()),
                end: (iteration % 3 != 1).then(|| at(end).to_rfc3339()),
                event_type: [None, Some("FILE_CREATED"), Some("MISSING_TYPE")][iteration % 3],
                buckets: random.gen_range(1..=180),
            };
            assert_record_oracle(&facets(&active, &filter), &records, &filter);
        }
    }
}

#[test]
fn direct_facet_requests_preserve_the_bucket_limit() {
    let temporary = tempfile::TempDir::new().unwrap();
    let active = prepare_case(
        &temporary,
        "facet-bucket-limit",
        &spread_records(),
        WriteMode::Counted,
    );
    let result = facets(
        &active,
        &FacetFilter {
            start: None,
            end: None,
            event_type: None,
            buckets: u32::MAX,
        },
    );
    assert_eq!(result.histogram.len(), 180);
}

#[test]
fn histogram_counts_match_uneven_integer_second_bucket_edges() {
    let temporary = tempfile::TempDir::new().unwrap();
    let records = (0..10)
        .map(|offset| {
            record(
                SOURCE,
                &format!("bucket-{offset}"),
                "FILE_CREATED",
                at(base_time().timestamp() + offset),
            )
        })
        .collect::<Vec<_>>();
    let active = prepare_case(
        &temporary,
        "facets-integer-buckets",
        &records,
        WriteMode::Counted,
    );
    let result = facets(
        &active,
        &FacetFilter {
            start: None,
            end: None,
            event_type: None,
            buckets: 3,
        },
    );
    assert_eq!(
        result
            .histogram
            .iter()
            .map(|bucket| bucket.count)
            .collect::<Vec<_>>(),
        [3, 3, 4]
    );
    for bucket in result.histogram {
        let first = DateTime::parse_from_rfc3339(&bucket.start_ts).unwrap();
        let last = DateTime::parse_from_rfc3339(&bucket.end_ts).unwrap();
        let count = records
            .iter()
            .filter(|record| {
                (first.timestamp()..=last.timestamp()).contains(&record.timestamp.timestamp())
            })
            .count();
        assert_eq!(bucket.count, count as u64);
    }
}
