use crate::import_pipeline::{emit, ImportEventSink, ImportProfiler};
use std::time::{Duration, Instant};
use transport::dto::{
    AnalysisExtractionPhaseDto, AnalysisExtractionProgressDto, AnalysisWorkStageDto,
};

pub(super) struct AnalysisAuditProgress<'a> {
    event_sink: Option<&'a dyn ImportEventSink>,
    job_id: &'a str,
    started: Instant,
    profiler: Option<&'a ImportProfiler>,
    last_phase: Option<AnalysisExtractionPhaseDto>,
    last_stage: Option<AnalysisWorkStageDto>,
    last_emit: Option<Instant>,
}

impl<'a> AnalysisAuditProgress<'a> {
    pub(super) fn new(
        event_sink: Option<&'a dyn ImportEventSink>,
        job_id: &'a str,
        started: Instant,
        profiler: Option<&'a ImportProfiler>,
    ) -> Self {
        Self {
            event_sink,
            job_id,
            started,
            profiler,
            last_phase: None,
            last_stage: None,
            last_emit: None,
        }
    }

    pub(super) fn emit(&mut self, progress: AnalysisExtractionProgressDto) {
        let stage = progress.work.as_ref().map(|work| work.stage);
        let terminal = matches!(
            progress.phase,
            AnalysisExtractionPhaseDto::Completed | AnalysisExtractionPhaseDto::Failed
        ) || progress.work.as_ref().is_some_and(|work| work.complete);
        let changed = self.last_phase != Some(progress.phase) || self.last_stage != stage;
        if !changed
            && !terminal
            && self
                .last_emit
                .is_some_and(|last| last.elapsed() < Duration::from_millis(250))
        {
            return;
        }
        self.last_phase = Some(progress.phase);
        self.last_stage = stage;
        self.last_emit = Some(Instant::now());
        if let Some(profiler) = self.profiler {
            let _ = profiler.record_progress(&progress);
        }
        let work = progress
            .work
            .as_ref()
            .map(|work| {
                format!(
                    " stage={:?} stageElapsedMs={} records={} bytes={} complete={}",
                    work.stage, work.elapsed_ms, work.records, work.bytes, work.complete,
                )
            })
            .unwrap_or_default();
        emit::emit_job_progress(self.event_sink, self.job_id, 96, &format!(
            "analysis-audit: phase={:?} category={} candidates={} processed={} artifacts={} timeline={} checkpoints={} elapsedMs={}{}",
            progress.phase, progress.category, progress.total_candidates, progress.processed_candidates,
            progress.artifact_count, progress.timeline_event_count, progress.checkpoint_hit_count,
            self.started.elapsed().as_millis(), work,
        ));
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/import_pipeline/analysis_progress.rs"]
mod tests;
