use domain::DataSourcePlatform;
use persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase;
use serde_json::json;

use super::{
    outcome::DerivedFinalizationReport, phase_execution::run_cancellable_phase,
    phase_runner::ProcessingPhaseRunner, DerivedSourceContext,
};
use crate::analysis_service::run_source_analysis_extraction_execution_with_cancel;

const MAX_PERSISTED_WARNING_DETAILS: usize = 100;

pub(super) fn run_artifact_phase(
    runner: &ProcessingPhaseRunner<'_>,
    context: &DerivedSourceContext<'_>,
    platform: DataSourcePlatform,
    report: &mut DerivedFinalizationReport,
) -> persistence_sqlite::repositories::processing_phase_repo::ProcessingPhaseState {
    run_cancellable_phase(
        runner,
        ProcessingPhase::Artifacts,
        report,
        context.cancel_token,
        || {
            let categories = categories_for(platform)?;
            let execution = run_source_analysis_extraction_execution_with_cancel(
                context.case_conn,
                context.case_root,
                context.case_id,
                context.data_source_id,
                &categories,
                context.cancel_token.clone(),
            )
            .map_err(|error| error.to_string())?;
            artifact_phase_output(execution)
        },
    )
}

pub(super) fn artifact_phase_output(
    execution: crate::analysis_service::AnalysisExtractionExecution,
) -> Result<String, String> {
    let extraction = execution.dto;
    let processing_rows_per_sec =
        rows_per_second(extraction.scanned_count, execution.processing_elapsed_ms);
    let source_read_avg_micros = average_micros(
        execution.source_read_elapsed_ms,
        execution.source_read_count,
    );
    let warning_details = extraction
        .warnings
        .iter()
        .take(MAX_PERSISTED_WARNING_DETAILS)
        .cloned()
        .collect::<Vec<_>>();
    if execution.retryable_failure_count > 0 {
        return Err(format!(
            "artifact extraction has {} retryable evidence-read failures: {}",
            execution.retryable_failure_count,
            warning_details.join(" | ")
        ));
    }
    Ok(json!({
        "status": extraction.status,
        "scannedCount": extraction.scanned_count,
        "checkpointHitCount": extraction.checkpoint_hit_count,
        "artifactCount": extraction.artifact_count,
        "timelineEventCount": extraction.timeline_event_count,
        "discoveryElapsedMs": execution.discovery_elapsed_ms,
        "processingElapsedMs": execution.processing_elapsed_ms,
        "processingRowsPerSec": processing_rows_per_sec,
        "sourceReadCount": execution.source_read_count,
        "sourceReadElapsedMs": execution.source_read_elapsed_ms,
        "sourceReadAvgMicros": source_read_avg_micros,
        "filesystemOpenOperations": execution.filesystem_read_metrics.filesystem_open_operations,
        "filesystemMetadataCacheHits": execution.filesystem_read_metrics.metadata_cache_hits,
        "filesystemMetadataCacheMisses": execution.filesystem_read_metrics.metadata_cache_misses,
        "filesystemEvidenceReadOperations": execution.filesystem_read_metrics.evidence_read_operations,
        "filesystemEvidenceBytesRead": execution.filesystem_read_metrics.evidence_bytes_read,
        "radosVerifiedCacheHits": execution.rados_read_metrics.verified_cache_hits,
        "radosVerifiedCacheMisses": execution.rados_read_metrics.verified_cache_misses,
        "radosPlanCacheHits": execution.rados_read_metrics.plan_cache_hits,
        "radosPlanCacheMisses": execution.rados_read_metrics.plan_cache_misses,
        "radosPlanLookupElapsedMicros": execution.rados_read_metrics.plan_lookup_elapsed_micros,
        "radosReadPlanSessionInitializations": execution.rados_read_metrics.read_plan_session_initializations,
        "radosReadPlanSessionElapsedMicros": execution.rados_read_metrics.read_plan_session_elapsed_micros,
        "radosReplicaDeviceReads": execution.rados_read_metrics.replica_device_reads,
        "radosReplicaDeviceBytes": execution.rados_read_metrics.replica_device_bytes,
        "radosReplicaDeviceElapsedMicros": execution.rados_read_metrics.replica_device_elapsed_micros,
        "persistenceElapsedMs": execution.persistence_elapsed_ms,
        "rssMb": crate::import_analysis::current_rss_mb(),
        "peakRssMb": crate::import_analysis::peak_rss_mb(),
        "warningCount": extraction.warnings.len(),
        "warningDetails": warning_details,
        "warningDetailsTruncated": extraction.warnings.len() > MAX_PERSISTED_WARNING_DETAILS,
    })
    .to_string())
}

fn rows_per_second(rows: u64, elapsed_ms: u64) -> u64 {
    if elapsed_ms == 0 {
        return rows;
    }
    rows.saturating_mul(1_000).saturating_add(elapsed_ms / 2) / elapsed_ms
}

fn average_micros(elapsed_ms: u64, count: u64) -> u64 {
    if count == 0 {
        return 0;
    }
    elapsed_ms.saturating_mul(1_000).saturating_add(count / 2) / count
}

fn categories_for(platform: DataSourcePlatform) -> Result<Vec<&'static str>, String> {
    match platform {
        DataSourcePlatform::Linux => Ok(vec!["LinuxArtifacts"]),
        DataSourcePlatform::Windows => Ok(Vec::new()),
        DataSourcePlatform::Unknown => {
            Err("unknown guest platform cannot run artifact extraction".to_string())
        }
    }
}
