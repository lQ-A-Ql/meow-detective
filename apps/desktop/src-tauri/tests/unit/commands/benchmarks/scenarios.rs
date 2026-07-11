use app_services::{search_service, timeline_service};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, file_repo::FileRepo};

use super::fixture::{first_data_source_id, setup_case};
use super::metrics::{measure_runs, scenario_result};

const WARMUP_RUNS: u32 = 2;
const MEASURE_RUNS: u32 = 5;

pub(super) fn run_all_scenarios() -> Vec<serde_json::Value> {
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

    vec![
        search_query(&source_index_dir),
        file_tree_expand(&source_connection),
        file_paginate(&source_connection),
        timeline_filter(&source_connection),
        artifact_extract(&source_connection),
        report_export(&active, &temporary),
    ]
}

fn search_query(source_index_dir: &std::path::Path) -> serde_json::Value {
    let marker = "fw_bench_search_a1b2c3";
    for _ in 0..WARMUP_RUNS {
        let _ = search_service::search_files_real(source_index_dir, marker, 0, 10);
    }
    scenario_result(
        "search_query",
        measure_runs(MEASURE_RUNS, || {
            let _ = search_service::search_files_real(source_index_dir, marker, 0, 10);
        }),
    )
}

fn file_tree_expand(connection: &rusqlite::Connection) -> serde_json::Value {
    for _ in 0..WARMUP_RUNS {
        let _ = app_services::file_service::get_file_tree_real(connection);
    }
    scenario_result(
        "file_tree_expand",
        measure_runs(MEASURE_RUNS, || {
            let _ = app_services::file_service::get_file_tree_real(connection);
        }),
    )
}

fn file_paginate(connection: &rusqlite::Connection) -> serde_json::Value {
    for _ in 0..WARMUP_RUNS {
        let _ = FileRepo::new(connection).find_root_entries_page(0, 10);
    }
    scenario_result(
        "file_paginate",
        measure_runs(MEASURE_RUNS, || {
            let _ = FileRepo::new(connection).find_root_entries_page(0, 10);
        }),
    )
}

fn timeline_filter(connection: &rusqlite::Connection) -> serde_json::Value {
    let _ = timeline_service::ensure_macb_timeline_projected(connection);
    for _ in 0..WARMUP_RUNS {
        let _ = timeline_service::query_timeline(connection, 0, 20);
    }
    scenario_result(
        "timeline_filter",
        measure_runs(MEASURE_RUNS, || {
            let _ = timeline_service::query_timeline(connection, 0, 20);
        }),
    )
}

fn artifact_extract(connection: &rusqlite::Connection) -> serde_json::Value {
    let repository = ArtifactRepo::new(connection);
    for _ in 0..WARMUP_RUNS {
        let _ = repository.list_by_family(None);
    }
    scenario_result(
        "artifact_extract",
        measure_runs(MEASURE_RUNS, || {
            let _ = repository.list_by_family(None);
        }),
    )
}

fn report_export(
    active: &app_services::active_case::ActiveCase,
    temporary: &tempfile::TempDir,
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
            for _ in 0..WARMUP_RUNS {
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
            Ok(measure_runs(MEASURE_RUNS, || {
                let _ = app_services::report::generate_html_report(
                    connection,
                    &active.meta,
                    &output_dir,
                    &scope,
                );
            }))
        })
        .unwrap();
    scenario_result("report_export", timings)
}
