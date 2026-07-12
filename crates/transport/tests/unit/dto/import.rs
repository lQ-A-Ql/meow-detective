use super::*;

fn metrics() -> ImportPhaseMetricsDto {
    ImportPhaseMetricsDto {
        elapsed_ms: 250,
        rss_mb: 512,
        workers: 4,
        rows_processed: 10,
        rows_total: Some(20),
        rows_per_sec: Some(40.0),
        bytes_processed: 1024,
        bytes_total: Some(2048),
        mb_per_sec: Some(8.5),
        warnings: 1,
        skipped: 2,
        failed: 0,
    }
}

fn partial_result() -> PartialResultDto {
    PartialResultDto {
        kind: PartialResultKindDto::TimelineBuckets,
        scope_id: "case-1".to_string(),
        ready_count: 6,
        total_estimate: Some(12),
        query_key: "timeline:buckets:case-1".to_string(),
        freshness: ResultFreshnessDto::Partial,
    }
}

#[test]
fn import_phase_progress_serializes_design_contract_as_camel_case() {
    let dto = ImportPhaseProgressDto {
        job_id: "job-1".to_string(),
        case_id: "case-1".to_string(),
        data_source_id: Some("ds-1".to_string()),
        phase: ImportPhaseDto::MergeAnalysis,
        state: ImportPhaseStateDto::Partial,
        percent: 42,
        detail: "Merging worker output".to_string(),
        metrics: metrics(),
        partial_results: vec![partial_result()],
        cancellable: true,
        cancel_requested: false,
    };

    let value = serde_json::to_value(dto).expect("serialize progress");

    assert_eq!(value["jobId"], "job-1");
    assert_eq!(value["caseId"], "case-1");
    assert_eq!(value["dataSourceId"], "ds-1");
    assert_eq!(value["phase"], "mergeAnalysis");
    assert_eq!(value["state"], "partial");
    assert_eq!(value["percent"], 42);
    assert_eq!(value["detail"], "Merging worker output");
    assert_eq!(value["metrics"]["elapsedMs"], 250);
    assert_eq!(value["metrics"]["rssMb"], 512);
    assert_eq!(value["metrics"]["rowsProcessed"], 10);
    assert_eq!(value["metrics"]["rowsTotal"], 20);
    assert_eq!(value["metrics"]["rowsPerSec"], 40.0);
    assert_eq!(value["metrics"]["bytesProcessed"], 1024);
    assert_eq!(value["metrics"]["bytesTotal"], 2048);
    assert_eq!(value["metrics"]["mbPerSec"], 8.5);
    assert_eq!(value["metrics"]["warnings"], 1);
    assert_eq!(value["metrics"]["skipped"], 2);
    assert_eq!(value["metrics"]["failed"], 0);
    assert_eq!(value["partialResults"][0]["kind"], "timelineBuckets");
    assert_eq!(value["partialResults"][0]["scopeId"], "case-1");
    assert_eq!(value["partialResults"][0]["readyCount"], 6);
    assert_eq!(value["partialResults"][0]["totalEstimate"], 12);
    assert_eq!(
        value["partialResults"][0]["queryKey"],
        "timeline:buckets:case-1"
    );
    assert_eq!(value["partialResults"][0]["freshness"], "partial");
    assert_eq!(value["cancellable"], true);
    assert_eq!(value["cancelRequested"], false);
    assert!(value.get("progressPercent").is_none());
    assert!(value.get("message").is_none());
    assert!(value.get("updatedAt").is_none());
    assert!(value.get("job_id").is_none());
    assert!(value["metrics"].get("elapsed_ms").is_none());
}

#[test]
fn partial_result_base_dto_has_no_required_payload() {
    let dto = partial_result();

    let value = serde_json::to_value(dto).expect("serialize partial result");

    assert_eq!(value["kind"], "timelineBuckets");
    assert_eq!(value["scopeId"], "case-1");
    assert_eq!(value["readyCount"], 6);
    assert_eq!(value["totalEstimate"], 12);
    assert_eq!(value["queryKey"], "timeline:buckets:case-1");
    assert_eq!(value["freshness"], "partial");
    assert!(value.get("payload").is_none());
    assert!(value.get("jobId").is_none());
    assert!(value.get("scope_id").is_none());
}

#[test]
fn cancellation_contract_serializes_design_request_and_state() {
    let request = CancelJobRequestDto {
        job_id: "job-1".to_string(),
        reason: CancelReasonDto::MemoryLimit,
        drain_timeout_ms: 30_000,
    };
    let cancellation = JobCancellationDto {
        job_id: "job-1".to_string(),
        requested_at: Some("2026-06-05T00:02:00Z".to_string()),
        acknowledged_at: Some("2026-06-05T00:02:01Z".to_string()),
        state: CancellationStateDto::Draining,
        safe_to_close: false,
        detail: "Draining import workers".to_string(),
    };

    let request_value = serde_json::to_value(request).expect("serialize request");
    let cancellation_value = serde_json::to_value(cancellation).expect("serialize cancellation");

    assert_eq!(request_value["jobId"], "job-1");
    assert_eq!(request_value["reason"], "memoryLimit");
    assert_eq!(request_value["drainTimeoutMs"], 30_000);
    assert_eq!(cancellation_value["jobId"], "job-1");
    assert_eq!(cancellation_value["requestedAt"], "2026-06-05T00:02:00Z");
    assert_eq!(cancellation_value["acknowledgedAt"], "2026-06-05T00:02:01Z");
    assert_eq!(cancellation_value["state"], "draining");
    assert_eq!(cancellation_value["safeToClose"], false);
    assert_eq!(cancellation_value["detail"], "Draining import workers");
    assert!(request_value.get("requestedBy").is_none());
    assert!(cancellation_value.get("canResume").is_none());
}

fn performance_metric(key: &str, value: f64, unit: &str) -> PerformanceMetricDto {
    PerformanceMetricDto {
        key: key.to_string(),
        value,
        unit: unit.to_string(),
    }
}

#[test]
fn cache_and_performance_summaries_serialize_camel_case_contract() {
    let cache = IndexCacheStatusDto {
        cache_key: "case:files".to_string(),
        state: "warming".to_string(),
        indexed_count: 12,
        total_count: Some(20),
        updated_at: "2026-06-05T00:03:00Z".to_string(),
        message: None,
    };
    let report = PerformanceReportSummaryDto {
        report_id: "perf-1".to_string(),
        job_id: Some("job-1".to_string()),
        generated_at: "2026-06-05T00:04:00Z".to_string(),
        elapsed_ms: 3000,
        peak_memory_bytes: Some(65536),
        summary: "Import completed within budget".to_string(),
    };

    let cache_value = serde_json::to_value(cache).expect("serialize cache");
    let report_value = serde_json::to_value(report).expect("serialize report");

    assert_eq!(cache_value["cacheKey"], "case:files");
    assert_eq!(cache_value["state"], "warming");
    assert_eq!(cache_value["indexedCount"], 12);
    assert_eq!(cache_value["totalCount"], 20);
    assert_eq!(cache_value["updatedAt"], "2026-06-05T00:03:00Z");
    assert_eq!(report_value["reportId"], "perf-1");
    assert_eq!(report_value["jobId"], "job-1");
    assert_eq!(report_value["generatedAt"], "2026-06-05T00:04:00Z");
    assert_eq!(report_value["elapsedMs"], 3000);
    assert_eq!(report_value["peakMemoryBytes"], 65536);
    assert!(cache_value.get("cache_key").is_none());
}

#[test]
fn performance_report_serializes_bounded_metric_keys() {
    let report = PerformanceReportDto {
        summary: PerformanceReportSummaryDto {
            report_id: "perf-1".to_string(),
            job_id: None,
            generated_at: "2026-06-05T00:04:00Z".to_string(),
            elapsed_ms: 15,
            peak_memory_bytes: None,
            summary: "Timeline query returned 25 rows in 15 ms".to_string(),
        },
        metrics: vec![
            performance_metric("timeline.query.elapsedMs", 15.0, "ms"),
            performance_metric("timeline.query.rows", 25.0, "rows"),
            performance_metric("search.index.rowsPerSec", 500.0, "rows/s"),
        ],
    };

    let value = serde_json::to_value(report).expect("serialize performance report");

    assert_eq!(value["summary"]["reportId"], "perf-1");
    assert_eq!(value["summary"]["elapsedMs"], 15);
    assert_eq!(value["metrics"][0]["key"], "timeline.query.elapsedMs");
    assert_eq!(value["metrics"][0]["value"], 15.0);
    assert_eq!(value["metrics"][0]["unit"], "ms");
    assert_eq!(value["metrics"][1]["key"], "timeline.query.rows");
    assert_eq!(value["metrics"][2]["key"], "search.index.rowsPerSec");
    assert!(value.get("rawRows").is_none());
    assert!(value.get("filePaths").is_none());
    assert!(value["metrics"][0].get("metric_key").is_none());
}

#[test]
fn cache_status_contract_allows_observability_state_strings() {
    for state in [
        "reused",
        "warming",
        "ready",
        "stale",
        "invalidated",
        "deferred",
    ] {
        let dto = IndexCacheStatusDto {
            cache_key: format!("search:index:ds-{state}"),
            state: state.to_string(),
            indexed_count: 0,
            total_count: None,
            updated_at: "2026-06-05T00:03:00Z".to_string(),
            message: Some(format!("cache state {state}")),
        };

        let value = serde_json::to_value(dto).expect("serialize cache status");

        assert_eq!(value["state"], state);
        assert_eq!(value["cacheKey"], format!("search:index:ds-{state}"));
        assert_eq!(value["message"], format!("cache state {state}"));
        assert!(value.get("stateReason").is_none());
        assert!(value.get("updated_at").is_none());
    }
}

#[test]
fn import_enums_accept_stable_lower_camel_case_design_values() {
    assert_eq!(
        serde_json::from_str::<ImportPhaseDto>("\"mergeEnumeration\"").unwrap(),
        ImportPhaseDto::MergeEnumeration
    );
    assert_eq!(
        serde_json::from_str::<ImportPhaseDto>("\"buildIndexes\"").unwrap(),
        ImportPhaseDto::BuildIndexes
    );
    assert_eq!(
        serde_json::from_str::<ImportPhaseStateDto>("\"cancelling\"").unwrap(),
        ImportPhaseStateDto::Cancelling
    );
    assert_eq!(
        serde_json::from_str::<PartialResultKindDto>("\"artifactFamily\"").unwrap(),
        PartialResultKindDto::ArtifactFamily
    );
    assert_eq!(
        serde_json::from_str::<PartialResultKindDto>("\"evidenceHash\"").unwrap(),
        PartialResultKindDto::EvidenceHash
    );
    assert_eq!(
        serde_json::from_str::<ResultFreshnessDto>("\"invalidated\"").unwrap(),
        ResultFreshnessDto::Invalidated
    );
    assert_eq!(
        serde_json::from_str::<CancelReasonDto>("\"caseClosing\"").unwrap(),
        CancelReasonDto::CaseClosing
    );
    assert_eq!(
        serde_json::from_str::<CancellationStateDto>("\"notRequested\"").unwrap(),
        CancellationStateDto::NotRequested
    );
    assert_eq!(
        serde_json::from_str::<CancellationStateDto>("\"timedOut\"").unwrap(),
        CancellationStateDto::TimedOut
    );
}
