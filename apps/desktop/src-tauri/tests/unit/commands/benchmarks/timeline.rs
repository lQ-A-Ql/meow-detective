use super::{
    config::RunConfig,
    metrics::{measure, ScenarioCounters},
    native_fixture::NativeFixture,
    native_records::{seed_timeline, TIMELINE_TYPE},
};
use app_services::timeline_service;

pub(super) fn cursor(config: &RunConfig, scenario: &str, page_size: u32) -> serde_json::Value {
    let fixture = NativeFixture::new(1, 1);
    seed_timeline(&fixture, config.scale.timeline_records);
    fixture.active.with_conn(|connection| {
        Ok(measure(config, scenario,
            serde_json::json!({"records": config.scale.timeline_records, "pageSize": page_size, "sources": 1}),
            || {
                let mut cursor = None;
                let mut pages = 0;
                let mut count = 0;
                let mut previous = None;
                loop {
                    let page = timeline_service::query_timeline_filtered_for_case(
                        connection, &fixture.active.case_root, &fixture.active.meta.id,
                        timeline_service::TimelineQuery {
                            offset: 0, limit: page_size, time_start: None, time_end: None,
                            event_type: Some(TIMELINE_TYPE), cursor: cursor.as_deref(),
                        },
                    ).unwrap();
                    for row in &page.items {
                        if let Some(previous) = &previous { assert!(previous >= &row.ts); }
                        previous = Some(row.ts.clone());
                    }
                    count += page.items.len();
                    pages += 1;
                    match page.next_cursor {
                        Some(next) => cursor = Some(next),
                        None => break,
                    }
                }
                assert_eq!(count, config.scale.timeline_records);
                ScenarioCounters { cursor_pages: Some(pages), returned_rows: Some(count as u64), ..Default::default() }
            },
        ))
    }).unwrap()
}

pub(super) fn facets(config: &RunConfig, scenario: &str, narrow: bool) -> serde_json::Value {
    let fixture = NativeFixture::new(1, 1);
    let count = config.scale.facet_records;
    seed_timeline(&fixture, count);
    let from_index = if narrow { count * 45 / 100 } else { 0 };
    let until_index = if narrow { count * 55 / 100 } else { count - 1 };
    let base = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z").unwrap();
    let from = (base + chrono::Duration::seconds(from_index as i64 * 60)).to_rfc3339();
    let until = (base + chrono::Duration::seconds(until_index as i64 * 60)).to_rfc3339();
    fixture.active.with_conn(|connection| {
        Ok(measure(config, scenario,
            serde_json::json!({"records": count, "matchingRecords": until_index - from_index + 1, "buckets": 64, "sources": 1, "requests": 1}),
            || {
                let result = timeline_service::get_timeline_facets_for_case(
                    connection, &fixture.active.case_root, &fixture.active.meta.id,
                    Some(&from), Some(&until), Some(TIMELINE_TYPE), 64,
                ).unwrap();
                assert_eq!(result.total_events, (until_index - from_index + 1) as u64);
                assert_eq!(result.histogram.iter().map(|bucket| bucket.count).sum::<u64>(), result.total_events);
                ScenarioCounters { returned_rows: Some(result.total_events), ..Default::default() }
            },
        ))
    }).unwrap()
}
