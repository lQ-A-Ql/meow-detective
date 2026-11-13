use super::*;

pub(super) fn record_preload_warnings(
    selected: &[AnalysisCapability],
    preloads: &RunPreloads,
    state: &mut ExtractionState,
) {
    if let Some(registry) = selected
        .iter()
        .find(|capability| capability.read_policy == CandidateReadPolicy::RegistryPreload)
    {
        for warning in preloads.registry.warnings.iter().cloned() {
            state.record_warning(*registry, warning);
        }
    }
    if let Some(browser) = selected
        .iter()
        .find(|capability| capability.key == "BrowserHistory")
    {
        for warning in preloads.browser.warnings.iter().cloned() {
            state.record_warning(*browser, warning);
        }
    }
    record_log_time_warnings(selected, &preloads.timezone_warnings, state);
}

fn record_log_time_warnings(
    selected: &[AnalysisCapability],
    warnings: &[String],
    state: &mut ExtractionState,
) {
    let Some(capability) = selected
        .iter()
        .find(|capability| capability.read_policy == CandidateReadPolicy::LinuxPathAware)
        .copied()
    else {
        return;
    };
    for warning in warnings {
        state.record_informational_warning(capability, warning.clone());
    }
}

pub(super) fn register_candidates(
    progress: &mut ExtractionProgressReporter<'_>,
    selected: &[AnalysisCapability],
    candidates: &[EvidenceCandidate],
) {
    for candidate in candidates {
        if let Some(capability) = capability_for_candidate(selected, candidate) {
            progress.register_candidate(capability, candidate);
        }
    }
}
