use super::emit::ImportEventSink;
use source_kernel::task_runtime::progress::TaskProgressReporter;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use transport::dto::{
    AnalysisExtractionProgressDto, DataSourceSummaryDto, ImportPhaseDto, ImportPhaseProgressDto,
    ImportPhaseStateDto, IndexCacheStatusDto, JobCancellationDto, PartialResultDto,
    TaskProgressDto, TaskProgressStateDto as State, TaskProgressUnitDto as Unit,
    TaskStageKeyDto as Stage,
};

pub(super) struct ObservedImportSink<'a> {
    downstream: Option<&'a dyn ImportEventSink>,
    reporter: Mutex<TaskProgressReporter>,
    categories: Mutex<BTreeMap<String, (u64, u64)>>,
    timeline_step: Mutex<Option<transport::dto::TaskProgressWorkDto>>,
    nested: bool,
}

impl<'a> ObservedImportSink<'a> {
    pub fn new(sink: Option<&'a dyn ImportEventSink>, job: &str, case: &str, nested: bool) -> Self {
        Self {
            downstream: sink,
            reporter: Mutex::new(TaskProgressReporter::new(job, case, nested)),
            categories: Mutex::new(BTreeMap::new()),
            timeline_step: Mutex::new(None),
            nested,
        }
    }

    fn emit(&self, progress: &TaskProgressDto) {
        if let Some(sink) = self.downstream {
            sink.task_progress(progress);
        }
    }

    pub fn heartbeat(&self, cancel: &AtomicBool) {
        let mut reporter = self.reporter.lock().unwrap();
        let progress = reporter.heartbeat(cancel.load(Ordering::Acquire));
        self.emit(&progress);
    }

    pub fn finish(&self, state: State) {
        let mut reporter = self.reporter.lock().unwrap();
        let progress = reporter.finish(state);
        self.emit(&progress);
    }

    pub fn catalog_only(&self) {
        let mut reporter = self.reporter.lock().unwrap();
        self.emit(&reporter.finish_stage(Stage::Analyze, true));
        if !self.nested {
            self.emit(&reporter.finish_stage(Stage::Index, true));
        }
    }

    fn stage_for_phase(&self, phase: &ImportPhaseDto) -> Stage {
        match phase {
            ImportPhaseDto::Queued | ImportPhaseDto::Attach => {
                if self.nested {
                    Stage::Validate
                } else {
                    Stage::Prepare
                }
            }
            ImportPhaseDto::Probe => {
                if self.nested {
                    Stage::Probe
                } else {
                    Stage::Prepare
                }
            }
            ImportPhaseDto::Enumerate | ImportPhaseDto::MergeEnumeration => Stage::Enumerate,
            ImportPhaseDto::Analyze
            | ImportPhaseDto::MergeAnalysis
            | ImportPhaseDto::Timeline
            | ImportPhaseDto::HashEvidence => {
                if self.nested {
                    Stage::Analyze
                } else {
                    Stage::Index
                }
            }
            ImportPhaseDto::PostReady => Stage::Analyze,
            ImportPhaseDto::BuildIndexes => Stage::Enumerate,
            ImportPhaseDto::Checkpoint | ImportPhaseDto::Finalize => Stage::Publish,
        }
    }
}

impl ImportEventSink for ObservedImportSink<'_> {
    fn timeline_work(
        &self,
        work: transport::dto::TaskProgressWorkDto,
        done: u64,
        total: Option<u64>,
    ) {
        let mut previous = self.timeline_step.lock().unwrap();
        let stage = if self.nested {
            Stage::Analyze
        } else {
            Stage::Index
        };
        let mut reporter = self.reporter.lock().unwrap();
        if *previous != Some(work) {
            *previous = Some(work);
            self.emit(&reporter.start_work(stage, Some(work)));
        }
        if let Some(event) = reporter.advance_work(done, total, Unit::Items, false) {
            self.emit(&event);
        }
    }
    fn task_progress(&self, progress: &TaskProgressDto) {
        self.emit(progress);
    }

    fn task_stage(&self, stage: Stage, complete: bool) {
        let mut reporter = self.reporter.lock().unwrap();
        let event = if complete {
            reporter.finish_stage(stage, false)
        } else {
            reporter.start(stage)
        };
        self.emit(&event);
    }

    fn job_progress(&self, job: &str, _percent: u32, detail: &str) {
        if let Some(sink) = self.downstream {
            let progress = self.reporter.lock().unwrap().progress();
            sink.job_progress(job, progress, detail);
        }
    }

    fn import_phase_progress(&self, phase: &ImportPhaseProgressDto) {
        let stage = self.stage_for_phase(&phase.phase);
        let mut reporter = self.reporter.lock().unwrap();
        let metrics = &phase.metrics;
        let (done, total, unit) = if metrics.bytes_total.is_some() {
            (metrics.bytes_processed, metrics.bytes_total, Unit::Bytes)
        } else {
            (metrics.rows_processed, metrics.rows_total, Unit::Items)
        };
        let mut event = reporter.observe(stage, done, total, unit);
        if phase.phase == ImportPhaseDto::Probe && phase.state == ImportPhaseStateDto::Completed {
            event = Some(reporter.finish_stage(stage, false));
        }
        let weighted = reporter.progress();
        if let Some(event) = &event {
            self.emit(event);
        }
        drop(reporter);
        if let Some(sink) = self.downstream {
            let mut phase = phase.clone();
            phase.percent = weighted;
            sink.import_phase_progress(&phase);
        }
    }

    fn analysis_progress(&self, progress: &AnalysisExtractionProgressDto) {
        let mut categories = self.categories.lock().unwrap();
        categories.insert(
            progress.category.clone(),
            (progress.processed_candidates, progress.total_candidates),
        );
        let completed = categories.values().map(|(done, _)| *done).sum();
        let total = categories.values().map(|(_, total)| *total).sum();
        let mut reporter = self.reporter.lock().unwrap();
        let event = reporter.advance(completed, Some(total), Unit::Items);
        if let Some(event) = event {
            self.emit(&event);
        }
    }

    fn partition_progress(
        &self,
        job: &str,
        partition: &str,
        completed: u32,
        total: u32,
        percent: u32,
    ) {
        if let Some(sink) = self.downstream {
            sink.partition_progress(job, partition, completed, total, percent);
        }
    }
    fn timeline_updated(&self, count: u64) {
        if let Some(sink) = self.downstream {
            sink.timeline_updated(count);
        }
    }
    fn search_index_progress(&self, progress: u32, detail: &str) {
        if let Some(sink) = self.downstream {
            sink.search_index_progress(progress, detail);
        }
    }
    fn data_source_imported(&self, case: &str, source: &DataSourceSummaryDto, job: &str) {
        if let Some(sink) = self.downstream {
            sink.data_source_imported(case, source, job);
        }
    }
    fn import_partial_result(&self, result: &PartialResultDto) {
        if let Some(sink) = self.downstream {
            sink.import_partial_result(result);
        }
    }
    fn cache_index_status(&self, status: &IndexCacheStatusDto) {
        if let Some(sink) = self.downstream {
            sink.cache_index_status(status);
        }
    }
    fn job_cancellation(&self, cancellation: &JobCancellationDto) {
        if let Some(sink) = self.downstream {
            sink.job_cancellation(cancellation);
        }
    }
}
