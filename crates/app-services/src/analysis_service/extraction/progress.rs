use super::linux::linux_candidate_support;
use super::linux_sections::LinuxCandidateSupport;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::capability::AnalysisCapability;
use domain::DataSourcePlatform;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use transport::dto::AnalysisExtractionPhaseDto;

const EVENT_INTERVAL: Duration = Duration::from_millis(100);
const LOG_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CandidateProgressResult {
    pub(crate) artifact_count: u64,
    pub(crate) timeline_event_count: u64,
    pub(crate) warning: bool,
    pub(crate) checkpoint_hit: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ExtractionProgressUpdate {
    pub(crate) category: String,
    pub(crate) label: String,
    pub(crate) phase: AnalysisExtractionPhaseDto,
    pub(crate) total_candidates: u64,
    pub(crate) processed_candidates: u64,
    pub(crate) structured_candidates: u64,
    pub(crate) unsupported_candidates: u64,
    pub(crate) text_fallback_candidates: u64,
    pub(crate) warning_candidates: u64,
    pub(crate) checkpoint_hit_count: u64,
    pub(crate) artifact_count: u64,
    pub(crate) timeline_event_count: u64,
    pub(crate) current_path: Option<String>,
    pub(crate) detail: String,
}

#[derive(Debug, Clone)]
struct SectionProgress {
    key: String,
    label: String,
    total_candidates: u64,
    processed_candidates: u64,
    structured_candidates: u64,
    unsupported_candidates: u64,
    text_fallback_candidates: u64,
    warning_candidates: u64,
    checkpoint_hit_count: u64,
    artifact_count: u64,
    timeline_event_count: u64,
    current_path: Option<String>,
}

impl SectionProgress {
    fn new(capability: AnalysisCapability) -> Self {
        Self {
            key: capability.key.to_string(),
            label: capability.section_label.to_string(),
            total_candidates: 0,
            processed_candidates: 0,
            structured_candidates: 0,
            unsupported_candidates: 0,
            text_fallback_candidates: 0,
            warning_candidates: 0,
            checkpoint_hit_count: 0,
            artifact_count: 0,
            timeline_event_count: 0,
            current_path: None,
        }
    }

    fn register(&mut self, support: CandidateSupport) {
        self.total_candidates = self.total_candidates.saturating_add(1);
        match support {
            CandidateSupport::Structured => {
                self.structured_candidates = self.structured_candidates.saturating_add(1)
            }
            CandidateSupport::TextFallback => {
                self.text_fallback_candidates = self.text_fallback_candidates.saturating_add(1)
            }
            CandidateSupport::Unsupported => {
                self.unsupported_candidates = self.unsupported_candidates.saturating_add(1)
            }
        }
    }

    fn record(&mut self, candidate: &EvidenceCandidate, result: CandidateProgressResult) {
        self.processed_candidates = self.processed_candidates.saturating_add(1);
        self.warning_candidates = self
            .warning_candidates
            .saturating_add(u64::from(result.warning));
        self.checkpoint_hit_count = self
            .checkpoint_hit_count
            .saturating_add(u64::from(result.checkpoint_hit));
        self.artifact_count = self.artifact_count.saturating_add(result.artifact_count);
        self.timeline_event_count = self
            .timeline_event_count
            .saturating_add(result.timeline_event_count);
        self.current_path = Some(candidate.path.clone());
    }

    fn update(
        &self,
        phase: AnalysisExtractionPhaseDto,
        detail: String,
    ) -> ExtractionProgressUpdate {
        ExtractionProgressUpdate {
            category: self.key.clone(),
            label: self.label.clone(),
            phase,
            total_candidates: self.total_candidates,
            processed_candidates: self.processed_candidates,
            structured_candidates: self.structured_candidates,
            unsupported_candidates: self.unsupported_candidates,
            text_fallback_candidates: self.text_fallback_candidates,
            warning_candidates: self.warning_candidates,
            checkpoint_hit_count: self.checkpoint_hit_count,
            artifact_count: self.artifact_count,
            timeline_event_count: self.timeline_event_count,
            current_path: self.current_path.clone(),
            detail,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateSupport {
    Structured,
    TextFallback,
    Unsupported,
}

pub(crate) struct ExtractionProgressReporter<'a> {
    platform: DataSourcePlatform,
    sections: BTreeMap<String, SectionProgress>,
    callback: &'a mut dyn FnMut(ExtractionProgressUpdate),
    last_event: Option<Instant>,
    last_log: Option<Instant>,
}

impl<'a> ExtractionProgressReporter<'a> {
    pub(crate) fn new(
        platform: DataSourcePlatform,
        selected: &[AnalysisCapability],
        callback: &'a mut dyn FnMut(ExtractionProgressUpdate),
    ) -> Self {
        let sections = selected
            .iter()
            .map(|capability| {
                (
                    capability.key.to_string(),
                    SectionProgress::new(*capability),
                )
            })
            .collect();
        Self {
            platform,
            sections,
            callback,
            last_event: None,
            last_log: None,
        }
    }

    pub(crate) fn emit_discovering(&mut self) {
        self.emit_all(
            AnalysisExtractionPhaseDto::Discovering,
            "discovering candidates",
        );
        tracing::info!(
            platform = ?self.platform,
            sections = self.sections.len(),
            "Analysis extraction discovery started"
        );
    }

    pub(crate) fn register_candidate(
        &mut self,
        capability: AnalysisCapability,
        candidate: &EvidenceCandidate,
    ) {
        let support = self.candidate_support(candidate);
        if let Some(section) = self.sections.get_mut(capability.key) {
            section.register(support);
        }
    }

    pub(crate) fn emit_preparing(&mut self) {
        self.emit_all(
            AnalysisExtractionPhaseDto::Preparing,
            "preparing extraction",
        );
        let totals = self
            .sections
            .values()
            .map(|section| section.total_candidates)
            .sum::<u64>();
        let unsupported = self
            .sections
            .values()
            .map(|section| section.unsupported_candidates)
            .sum::<u64>();
        let fallback = self
            .sections
            .values()
            .map(|section| section.text_fallback_candidates)
            .sum::<u64>();
        tracing::info!(
            total_candidates = totals,
            unsupported_candidates = unsupported,
            text_fallback_candidates = fallback,
            "Analysis extraction candidate inventory ready"
        );
    }

    pub(crate) fn emit_waiting_for_scheduler(&mut self, waited: Duration) {
        self.emit_all(
            AnalysisExtractionPhaseDto::Preparing,
            "waiting for the serial data-source extraction slot",
        );
        tracing::info!(
            waited_ms = waited.as_millis(),
            "Analysis extraction is waiting for the serial data-source slot"
        );
    }

    pub(crate) fn begin_extraction(&mut self) {
        self.emit_all(
            AnalysisExtractionPhaseDto::Extracting,
            "extracting candidates",
        );
    }

    pub(crate) fn start_candidate(
        &mut self,
        capability: AnalysisCapability,
        candidate: &EvidenceCandidate,
    ) {
        let support = self.candidate_support(candidate);
        let Some(section) = self.sections.get_mut(capability.key) else {
            return;
        };
        section.current_path = Some(candidate.path.clone());
        // Emit before reading so a slow evidence source is visible instead of
        // looking like a stalled extractor.
        self.emit_section_if_due_with_detail(
            capability.key,
            true,
            format!("reading candidate ({})", support.as_label()),
        );
        if support != CandidateSupport::Structured {
            tracing::info!(
                category = %capability.key,
                support = support.as_label(),
                current_path = %candidate.path,
                "Linux candidate uses a non-structured extraction path"
            );
        }
    }

    pub(crate) fn report_read_progress(
        &mut self,
        capability: AnalysisCapability,
        candidate: &EvidenceCandidate,
        bytes_read: usize,
        read_limit: usize,
    ) {
        let support = self.candidate_support(candidate);
        self.emit_section_if_due_with_detail(
            capability.key,
            false,
            format!(
                "reading candidate ({}) {bytes_read}/{read_limit} bytes",
                support.as_label()
            ),
        );
    }

    pub(crate) fn finish_candidate(
        &mut self,
        capability: AnalysisCapability,
        candidate: &EvidenceCandidate,
        result: CandidateProgressResult,
    ) {
        let (processed_candidates, total_candidates, force) = {
            let Some(section) = self.sections.get_mut(capability.key) else {
                return;
            };
            section.record(candidate, result);
            (
                section.processed_candidates,
                section.total_candidates,
                section.processed_candidates >= section.total_candidates,
            )
        };
        let support = self.candidate_support(candidate);
        let outcome = if result.warning {
            "warning"
        } else {
            "complete"
        };
        self.emit_section_if_due_with_detail(
            capability.key,
            force,
            format!(
                "processed {}/{} candidate(s) ({}, {outcome})",
                processed_candidates,
                total_candidates,
                support.as_label()
            ),
        );
    }

    pub(crate) fn begin_persisting(&mut self) {
        self.emit_all(
            AnalysisExtractionPhaseDto::Persisting,
            "persisting extracted artifacts",
        );
    }

    pub(crate) fn complete(&mut self) {
        self.emit_all(
            AnalysisExtractionPhaseDto::Completed,
            "extraction completed",
        );
        let processed = self
            .sections
            .values()
            .map(|section| section.processed_candidates)
            .sum::<u64>();
        let total = self
            .sections
            .values()
            .map(|section| section.total_candidates)
            .sum::<u64>();
        tracing::info!(
            processed_candidates = processed,
            total_candidates = total,
            "Analysis extraction completed"
        );
    }

    fn candidate_support(&self, candidate: &EvidenceCandidate) -> CandidateSupport {
        if self.platform != DataSourcePlatform::Linux {
            return CandidateSupport::Structured;
        }
        match linux_candidate_support(&normalize_evidence_path(&candidate.path)) {
            LinuxCandidateSupport::Structured => CandidateSupport::Structured,
            LinuxCandidateSupport::TextFallback => CandidateSupport::TextFallback,
            LinuxCandidateSupport::Unsupported => CandidateSupport::Unsupported,
        }
    }

    fn emit_all(&mut self, phase: AnalysisExtractionPhaseDto, detail: &str) {
        let keys = self.sections.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            self.emit_section(&key, phase, detail.to_string());
        }
    }

    fn emit_section_if_due_with_detail(&mut self, key: &str, force: bool, detail: String) {
        let due = force
            || self
                .last_event
                .is_none_or(|last| last.elapsed() >= EVENT_INTERVAL);
        if !due {
            return;
        }
        self.emit_section(key, AnalysisExtractionPhaseDto::Extracting, detail);
    }

    fn emit_section(&mut self, key: &str, phase: AnalysisExtractionPhaseDto, detail: String) {
        let Some(section) = self.sections.get(key) else {
            return;
        };
        let update = section.update(phase, detail);
        let should_log = self
            .last_log
            .is_none_or(|last| last.elapsed() >= LOG_INTERVAL)
            || update.phase == AnalysisExtractionPhaseDto::Completed;
        (self.callback)(update.clone());
        self.last_event = Some(Instant::now());
        if should_log {
            tracing::info!(
                category = %update.category,
                processed_candidates = update.processed_candidates,
                total_candidates = update.total_candidates,
                current_path = update.current_path.as_deref().unwrap_or(""),
                "Analysis extraction progress"
            );
            self.last_log = Some(Instant::now());
        }
    }
}

impl CandidateSupport {
    fn as_label(self) -> &'static str {
        match self {
            Self::Structured => "structured",
            Self::TextFallback => "text-fallback",
            Self::Unsupported => "unsupported",
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/progress.rs"]
mod tests;
