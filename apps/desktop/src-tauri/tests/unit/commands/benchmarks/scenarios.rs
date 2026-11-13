use super::{config::RunConfig, core_scenarios, native_scenarios};

pub(super) fn run_all_scenarios() -> Vec<serde_json::Value> {
    let config = RunConfig::from_env();
    config
        .selected
        .iter()
        .map(|scenario| match scenario.as_str() {
            "search_query" | "file_tree_expand" | "file_paginate" | "timeline_filter"
            | "artifact_list" | "report_export" => core_scenarios::run(&config, scenario),
            _ => native_scenarios::run(&config, scenario),
        })
        .collect()
}
