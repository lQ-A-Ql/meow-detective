use std::path::Path;

use app_services::{search_service, timeline_service};
use persistence_source::repositories::{artifact_repo::ArtifactRepo, file_repo::FileRepo};

use super::fixture::{first_data_source_id, setup_case};
use super::metrics::{measure_runs, measure_runs_with_counters, scenario_result, ScenarioCounters};

const SCENARIO_ALL: &str = "all";
const LEGACY_SCENARIO_COUNT: usize = 6;
const ALL_SCENARIO_NAMES: [&str; 10] = [
    "search_query",
    "file_tree_expand",
    "file_paginate",
    "timeline_filter",
    "artifact_extract",
    "report_export",
    "timeline_native_cursor_p50",
    "timeline_native_cursor_p256",
    "timeline_native_facets_narrow",
    "timeline_native_facets_full",
];

const DEFAULT_MEASURE_RUNS: u32 = 5;
const MIN_MEASURE_RUNS: u32 = 1;
const MAX_MEASURE_RUNS: u32 = 100;
const DEFAULT_WARMUP_RUNS: u32 = 2;
const MIN_WARMUP_RUNS: u32 = 0;
const MAX_WARMUP_RUNS: u32 = 10;

const CURSOR_PAGE_P50: u32 = 50;
const CURSOR_PAGE_P256: u32 = 256;
const FACET_BUCKET_COUNT: u32 = 64;

const FIXTURE_TOO_SMALL: &str = "fixture too small; use -FixtureLevel medium or large";

pub(super) fn run_all_scenarios() -> Vec<serde_json::Value> {
    let requested =
        std::env::var("FORENSICS_BENCH_SCENARIO").unwrap_or_else(|_| SCENARIO_ALL.to_owned());
    let runs = env_u32(
        "FORENSICS_BENCH_RUNS",
        DEFAULT_MEASURE_RUNS,
        MIN_MEASURE_RUNS,
        MAX_MEASURE_RUNS,
    );
    let warmup_runs = env_u32(
        "FORENSICS_BENCH_WARMUP_RUNS",
        DEFAULT_WARMUP_RUNS,
        MIN_WARMUP_RUNS,
        MAX_WARMUP_RUNS,
    );
    // Validate the requested scenario before paying for case setup.
    let selected = select_scenarios(requested.trim());

    let (active, temporary) = setup_case();
    let data_source_id = active
        .with_conn(|connection| Ok(first_data_source_id(connection, &active.meta.id)))
        .unwrap();
    let source_connection =
        app_services::source_db::open_source_db(&active.case_root, &data_source_id)
            .expect("benchmark source DB should exist");
    let source_index_dir =
        app_services::source_db::source_index_dir(&active.case_root, &data_source_id);

    let file_count = FileRepo::new(&source_connection).count_all().unwrap();
    assert!(file_count >= 2, "Expected file entries, got {file_count}");
    let artifact_count = ArtifactRepo::new(&source_connection).count().unwrap();
    assert!(
        artifact_count >= 1,
        "Expected at least 1 artifact, got {artifact_count}"
    );

    let mut results = Vec::with_capacity(selected.len());
    for scenario in selected {
        results.push(run_scenario(
            scenario,
            &active,
            &temporary,
            &source_connection,
            &source_index_dir,
            runs,
            warmup_runs,
        ));
    }
    results
}

fn select_scenarios(requested: &str) -> Vec<&'static str> {
    if requested == SCENARIO_ALL {
        return ALL_SCENARIO_NAMES[..LEGACY_SCENARIO_COUNT].to_vec();
    }
    match ALL_SCENARIO_NAMES
        .iter()
        .position(|name| *name == requested)
    {
        Some(index) => vec![ALL_SCENARIO_NAMES[index]],
        None => panic!(
            "unknown benchmark scenario '{requested}'; valid names: {SCENARIO_ALL}, {}",
            ALL_SCENARIO_NAMES.join(", ")
        ),
    }
}

fn env_u32(name: &str, default: u32, min: u32, max: u32) -> u32 {
    let Ok(raw) = std::env::var(name) else {
        return default;
    };
    let parsed = match raw.trim().parse::<u32>() {
        Ok(value) => value,
        Err(_) => panic!("{name} must be an unsigned integer, got '{raw}'"),
    };
    parsed.clamp(min, max)
}

fn run_scenario(
    scenario: &str,
    active: &app_services::active_case::ActiveCase,
    temporary: &tempfile::TempDir,
    source_connection: &rusqlite::Connection,
    source_index_dir: &Path,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    match scenario {
        "search_query" => search_query(source_index_dir, runs, warmup_runs),
        "file_tree_expand" => file_tree_expand(source_connection, runs, warmup_runs),
        "file_paginate" => file_paginate(source_connection, runs, warmup_runs),
        "timeline_filter" => timeline_filter(source_connection, runs, warmup_runs),
        "artifact_extract" => artifact_extract(source_connection, runs, warmup_runs),
        "report_export" => report_export(active, temporary, runs, warmup_runs),
        "timeline_native_cursor_p50" => {
            timeline_native_cursor(active, scenario, CURSOR_PAGE_P50, runs, warmup_runs)
        }
        "timeline_native_cursor_p256" => {
            timeline_native_cursor(active, scenario, CURSOR_PAGE_P256, runs, warmup_runs)
        }
        "timeline_native_facets_narrow" => timeline_native_facets_narrow(active, runs, warmup_runs),
        "timeline_native_facets_full" => timeline_native_facets_full(active, runs, warmup_runs),
        other => panic!("unhandled benchmark scenario '{other}'"),
    }
}

fn search_query(source_index_dir: &Path, runs: u32, warmup_runs: u32) -> serde_json::Value {
    let marker = "fw_bench_search_a1b2c3";
    for _ in 0..warmup_runs {
        let _ = search_service::search_files_real(source_index_dir, marker, 0, 10);
    }
    let timings = measure_runs(runs, || {
        let _ = search_service::search_files_real(source_index_dir, marker, 0, 10);
    });
    scenario_result(
        "search_query",
        timings,
        warmup_runs,
        ScenarioCounters::default(),
    )
}

fn file_tree_expand(
    connection: &rusqlite::Connection,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    for _ in 0..warmup_runs {
        let _ = app_services::file_service::get_file_tree_real(connection);
    }
    let timings = measure_runs(runs, || {
        let _ = app_services::file_service::get_file_tree_real(connection);
    });
    scenario_result(
        "file_tree_expand",
        timings,
        warmup_runs,
        ScenarioCounters::default(),
    )
}

fn file_paginate(
    connection: &rusqlite::Connection,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    for _ in 0..warmup_runs {
        let _ = FileRepo::new(connection).find_root_entries_page(0, 10);
    }
    let timings = measure_runs(runs, || {
        let _ = FileRepo::new(connection).find_root_entries_page(0, 10);
    });
    scenario_result(
        "file_paginate",
        timings,
        warmup_runs,
        ScenarioCounters::default(),
    )
}

fn timeline_filter(
    connection: &rusqlite::Connection,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    // Legacy behavior: project the SQL timeline once, before the warmups.
    let _ = timeline_service::materialize_file_activity_unknown(connection);
    for _ in 0..warmup_runs {
        let _ = timeline_service::query_timeline(connection, 0, 20);
    }
    let timings = measure_runs(runs, || {
        let _ = timeline_service::query_timeline(connection, 0, 20);
    });
    scenario_result(
        "timeline_filter",
        timings,
        warmup_runs,
        ScenarioCounters::default(),
    )
}

fn artifact_extract(
    connection: &rusqlite::Connection,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    let repository = ArtifactRepo::new(connection);
    for _ in 0..warmup_runs {
        let _ = repository.list_by_family(None);
    }
    let timings = measure_runs(runs, || {
        let _ = repository.list_by_family(None);
    });
    scenario_result(
        "artifact_extract",
        timings,
        warmup_runs,
        ScenarioCounters::default(),
    )
}

fn report_export(
    active: &app_services::active_case::ActiveCase,
    temporary: &tempfile::TempDir,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    let output_dir = temporary.path().join("reports");
    std::fs::create_dir_all(&output_dir).unwrap();
    let scope = transport::commands::ExportScopeDto {
        file_system_metadata: true,
        registry: true,
        full_timeline: true,
        raw_file_extraction: false,
        overwrite: true,
    };

    active
        .with_conn(|connection| {
            for _ in 0..warmup_runs {
                let _ = app_services::report::generate_html_report(
                    connection,
                    &active.meta,
                    &output_dir,
                    &scope,
                );
            }
            Ok(())
        })
        .unwrap();
    let timings = active
        .with_conn(|connection| {
            Ok(measure_runs(runs, || {
                let _ = app_services::report::generate_html_report(
                    connection,
                    &active.meta,
                    &output_dir,
                    &scope,
                );
            }))
        })
        .unwrap();
    scenario_result(
        "report_export",
        timings,
        warmup_runs,
        ScenarioCounters::default(),
    )
}

fn timeline_native_cursor(
    active: &app_services::active_case::ActiveCase,
    scenario: &str,
    page_size: u32,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    let case_root = active.case_root.as_path();
    let case_id = &active.meta.id;
    active
        .with_conn(|connection| {
            for _ in 0..warmup_runs {
                timeline_cursor_pass(connection, case_root, case_id, page_size);
            }
            let mut last_pages = 0u64;
            let timings = measure_runs(runs, || {
                last_pages = timeline_cursor_pass(connection, case_root, case_id, page_size);
            });
            let counters = ScenarioCounters {
                cursor_pages: Some(last_pages),
                ..ScenarioCounters::default()
            };
            Ok(scenario_result(scenario, timings, warmup_runs, counters))
        })
        .unwrap()
}

/// Pages through the whole timeline with cursor continuation and returns the
/// number of pages fetched.
fn timeline_cursor_pass(
    connection: &rusqlite::Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    page_size: u32,
) -> u64 {
    let mut cursor: Option<String> = None;
    let mut pages = 0u64;
    loop {
        let query = timeline_service::TimelineQuery {
            offset: 0,
            limit: page_size,
            time_start: None,
            time_end: None,
            event_type: None,
            cursor: cursor.as_deref(),
        };
        let result = timeline_service::query_timeline_filtered_for_case(
            connection, case_root, case_id, query,
        );
        let page = result.expect("benchmark timeline cursor query failed");
        if pages == 0 && page.items.is_empty() {
            panic!("{FIXTURE_TOO_SMALL} (timeline cursor query returned no events)");
        }
        pages = pages.saturating_add(1);
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    pages
}

fn timeline_native_facets_full(
    active: &app_services::active_case::ActiveCase,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    let case_root = active.case_root.as_path();
    let case_id = &active.meta.id;
    active
        .with_conn(|connection| {
            for _ in 0..warmup_runs {
                let _ = query_timeline_facets(connection, case_root, case_id, None, None);
            }
            let (timings, counters) = measure_runs_with_counters(runs, || {
                let _ = query_timeline_facets(connection, case_root, case_id, None, None);
            });
            Ok(scenario_result(
                "timeline_native_facets_full",
                timings,
                warmup_runs,
                counters,
            ))
        })
        .unwrap()
}

fn timeline_native_facets_narrow(
    active: &app_services::active_case::ActiveCase,
    runs: u32,
    warmup_runs: u32,
) -> serde_json::Value {
    let case_root = active.case_root.as_path();
    let case_id = &active.meta.id;
    active
        .with_conn(|connection| {
            let overview = query_timeline_facets(connection, case_root, case_id, None, None);
            let (narrow_start, narrow_end) = middle_tenth_window(&overview);
            for _ in 0..warmup_runs {
                let _ = query_timeline_facets(
                    connection,
                    case_root,
                    case_id,
                    Some(narrow_start.as_str()),
                    Some(narrow_end.as_str()),
                );
            }
            let (timings, counters) = measure_runs_with_counters(runs, || {
                let _ = query_timeline_facets(
                    connection,
                    case_root,
                    case_id,
                    Some(narrow_start.as_str()),
                    Some(narrow_end.as_str()),
                );
            });
            Ok(scenario_result(
                "timeline_native_facets_narrow",
                timings,
                warmup_runs,
                counters,
            ))
        })
        .unwrap()
}

fn query_timeline_facets(
    connection: &rusqlite::Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    time_start: Option<&str>,
    time_end: Option<&str>,
) -> transport::dto::TimelineFacetsDto {
    let result = timeline_service::get_timeline_facets_for_case(
        connection,
        case_root,
        case_id,
        time_start,
        time_end,
        None,
        FACET_BUCKET_COUNT,
    );
    result.expect("benchmark timeline facets query failed")
}

/// Returns the middle 10% of the observed timeline window as RFC3339 bounds.
fn middle_tenth_window(facets: &transport::dto::TimelineFacetsDto) -> (String, String) {
    let start_epoch = facet_boundary(facets.start_ts.as_deref());
    let end_epoch = facet_boundary(facets.end_ts.as_deref());
    let span = end_epoch.saturating_sub(start_epoch);
    // 5% of the span, so 9 steps and 11 steps bracket the 45%..55% window.
    let step = span / 20;
    let narrow_start = start_epoch.saturating_add(step.saturating_mul(9));
    let narrow_end = start_epoch.saturating_add(step.saturating_mul(11));
    (
        rfc3339_from_epoch(narrow_start),
        rfc3339_from_epoch(narrow_end),
    )
}

fn facet_boundary(value: Option<&str>) -> i64 {
    let Some(raw) = value else {
        panic!("{FIXTURE_TOO_SMALL} (timeline facets reported no timestamp bounds)");
    };
    chrono::DateTime::parse_from_rfc3339(raw)
        .expect("timeline facet timestamp must be RFC3339")
        .timestamp()
}

fn rfc3339_from_epoch(epoch: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(epoch, 0)
        .expect("timeline facet window must be representable")
        .to_rfc3339()
}
