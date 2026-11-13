use super::{artifact, clone_store, config::RunConfig, correlation, headers, search, timeline};

pub(super) fn run(config: &RunConfig, scenario: &str) -> serde_json::Value {
    match scenario {
        "reprocess_generation_clone" => clone_store::run(config),
        "timeline_native_cursor_p50" => timeline::cursor(config, scenario, 50),
        "timeline_native_cursor_p256" => timeline::cursor(config, scenario, 256),
        "timeline_native_facets_narrow" => timeline::facets(config, scenario, true),
        "timeline_native_facets_full" => timeline::facets(config, scenario, false),
        "artifact_native_facets" => artifact::run(config),
        "case_search_cross_source" => search::run(config, scenario, false),
        "case_search_cross_source_offset" => search::run(config, scenario, true),
        "correlation_paths" => correlation::run(config),
        "file_header_reads" => headers::run(config),
        _ => panic!("unhandled benchmark scenario: {scenario}"),
    }
}
