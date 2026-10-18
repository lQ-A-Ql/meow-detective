pub(super) fn requested() -> app_services::timeline_service::TimelineProjectionPolicy {
    let mut policy = app_services::timeline_service::TimelineProjectionPolicy::default();
    if let Ok(value) = std::env::var("FORENSICS_TIMELINE_GRAPH_BATCH") {
        policy.graph_batch_rows = value.parse().expect("graph batch rows");
    }
    if let Ok(value) = std::env::var("FORENSICS_TIMELINE_CACHED_STATEMENTS") {
        policy.cached_graph_statements = value.parse().expect("cached statement bool");
    }
    if let Ok(value) = std::env::var("FORENSICS_TIMELINE_WAL_PAGES") {
        policy.wal_autocheckpoint_pages = Some(value.parse().expect("WAL pages"));
    }
    if let Ok(value) = std::env::var("FORENSICS_TIMELINE_CACHE_KIB") {
        policy.cache_kib = value.parse().expect("cache KiB");
    }
    policy.validate().expect("bounded projection policy");
    policy
}

pub(super) fn describe() -> serde_json::Value {
    let policy = requested();
    serde_json::json!({"graphBatchRows":policy.graph_batch_rows,"cacheKiB":policy.cache_kib,
        "cachedGraphStatements":policy.cached_graph_statements,
        "walAutocheckpointPages":policy.wal_autocheckpoint_pages})
}
