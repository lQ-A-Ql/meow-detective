use super::*;

pub(super) fn extract_bytes(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    browser_preload: &BrowserPreloadContext,
    linux_log_time: &LinuxLogTimeContext,
) -> ExtractionOutcome {
    match candidate.category.as_str() {
        "BrowserHistory" => extract_browser_candidate(candidate, bytes, browser_preload),
        "Email" => extract_email_candidate(candidate, bytes),
        "ProgramExecution" | "UserActivity" | "RecycleBin" | "Thumbnails" | "ResourceUsage" => {
            super::super::windows_native::extract(candidate, bytes)
        }
        LINUX_UMBRELLA_KEY => extract_linux_candidate_with_time(candidate, bytes, linux_log_time),
        _ => ExtractionOutcome::default(),
    }
}

pub(super) fn persist_candidate_outcome(
    output: &StreamingGeneration,
    outcome: ExtractionOutcome,
    plugin_failures: Vec<super::super::PluginExtractFailure>,
) -> CandidateCompletionKind {
    match output.persist_outcome(outcome) {
        Ok(observation) => CandidateCompletionKind::Persisted {
            observation,
            plugin_failures,
        },
        Err(error) => CandidateCompletionKind::Failed(error),
    }
}

pub(super) fn is_unsupported_linux_candidate(candidate: &EvidenceCandidate) -> bool {
    candidate.category == LINUX_UMBRELLA_KEY
        && linux_candidate_support(&normalize_evidence_path(&candidate.path))
            == LinuxCandidateSupport::Unsupported
}

pub(super) fn log_scheduler_snapshot(
    snapshot: SchedulerSnapshot,
    policy: ExtractionSchedulingPolicy,
) {
    tracing::info!(
        submitted = snapshot.submitted,
        completed = snapshot.completed,
        active_or_queued = snapshot.in_flight_items,
        in_flight_bytes = snapshot.in_flight_bytes,
        worker_budget = policy.worker_count,
        rss_mb = source_kernel::runtime_resources::current_rss_mb(),
        "Analysis extraction scheduler heartbeat"
    );
}
