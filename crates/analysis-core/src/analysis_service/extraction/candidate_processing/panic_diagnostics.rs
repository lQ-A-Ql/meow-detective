use super::{CandidateProcessingContext, CandidateWorkItem};
use crate::analysis_service::capability::AnalysisCapability;
use crate::analysis_service::extraction::state::ExtractionState;
use crate::analysis_service::{AnalysisServiceError, EvidenceCandidate};
use std::sync::Mutex;

/// Collects candidate-level diagnostics for panicked (or otherwise lost)
/// scheduler items so a single bad parser degrades into a warning instead of
/// aborting the whole extraction run.
pub(super) struct PanicDiagnostics {
    /// Submission-order index mapping scheduler sequences to candidates.
    index: Vec<(AnalysisCapability, EvidenceCandidate)>,
    sink: Mutex<Vec<(Option<usize>, AnalysisCapability, String)>>,
    fallback_capability: Option<AnalysisCapability>,
}

impl PanicDiagnostics {
    pub(super) fn new(items: &[CandidateWorkItem], selected: &[AnalysisCapability]) -> Self {
        Self {
            index: items
                .iter()
                .map(|item| (item.capability, item.candidate.clone()))
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
            Some((capability, candidate)) => Some((
                Some(sequence),
                *capability,
                format!(
                    "{} analysis failed and was skipped: {summary}",
                    candidate.path
                ),
            )),
            None => self.fallback_capability.map(|capability| {
                (
                    None,
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

    pub(super) fn drain_into(
        self,
        state: &mut ExtractionState,
        context: &CandidateProcessingContext<'_>,
    ) -> Result<(), AnalysisServiceError> {
        for (sequence, capability, warning) in self
            .sink
            .into_inner()
            .expect("panic diagnostics mutex poisoned")
        {
            let Some((_, candidate)) = sequence.and_then(|index| self.index.get(index)) else {
                return Err(AnalysisServiceError::Extraction(warning));
            };
            super::output::preserve_failed_candidate(context, candidate, capability)?;
            state.record_warning(capability, warning);
        }
        Ok(())
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
