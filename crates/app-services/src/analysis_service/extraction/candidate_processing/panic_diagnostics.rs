use super::CandidateWorkItem;
use crate::analysis_service::capability::AnalysisCapability;
use crate::analysis_service::extraction::state::ExtractionState;
use std::sync::Mutex;

/// Collects candidate-level diagnostics for panicked (or otherwise lost)
/// scheduler items so a single bad parser degrades into a warning instead of
/// aborting the whole extraction run.
pub(super) struct PanicDiagnostics {
    /// Submission-order index mapping scheduler sequences to candidates.
    index: Vec<(AnalysisCapability, String)>,
    sink: Mutex<Vec<(AnalysisCapability, String)>>,
    fallback_capability: Option<AnalysisCapability>,
}

impl PanicDiagnostics {
    pub(super) fn new(items: &[CandidateWorkItem], selected: &[AnalysisCapability]) -> Self {
        Self {
            index: items
                .iter()
                .map(|item| (item.capability, item.candidate.path.clone()))
                .collect(),
            sink: Mutex::new(Vec::new()),
            fallback_capability: selected.first().copied(),
        }
    }

    /// `sequence` indexes submission order for candidate panics; worker-level
    /// failures pass a worker id that may not resolve, in which case the
    /// diagnostic is attributed to the fallback capability without a path.
    pub(super) fn record(&self, sequence: usize, message: &str) {
        let summary = summarize_panic_message(message);
        let entry = match self.index.get(sequence) {
            Some((capability, path)) => Some((
                *capability,
                format!("{path} parser panicked and was skipped: {summary}"),
            )),
            None => self.fallback_capability.map(|capability| {
                (
                    capability,
                    format!("analysis worker {sequence} terminated unexpectedly: {summary}"),
                )
            }),
        };
        if let Some(entry) = entry {
            self.sink
                .lock()
                .expect("panic diagnostics mutex poisoned")
                .push(entry);
        }
    }

    pub(super) fn drain_into(self, state: &mut ExtractionState) {
        for (capability, warning) in self
            .sink
            .into_inner()
            .expect("panic diagnostics mutex poisoned")
        {
            state.record_warning(capability, warning);
        }
    }
}

fn summarize_panic_message(message: &str) -> String {
    const MAX_PANIC_SUMMARY_CHARS: usize = 160;
    if message.chars().count() <= MAX_PANIC_SUMMARY_CHARS {
        message.to_string()
    } else {
        format!(
            "{}…",
            message
                .chars()
                .take(MAX_PANIC_SUMMARY_CHARS)
                .collect::<String>()
        )
    }
}
