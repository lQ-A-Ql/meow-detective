use super::{events, CapabilityWork, ExecutableCapability, PostImportError};
use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRow;
use source_kernel::task_runtime::progress::TaskProgressReporter;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use transport::dto::{
    AnalysisExtractionProgressDto, TaskProgressStateDto as State, TaskProgressUnitDto as Unit,
    TaskStageKeyDto as Stage,
};
use transport::events::EventTopic;

pub(super) struct CapabilityProgress {
    reporter: Mutex<TaskProgressReporter>,
    categories: Mutex<BTreeMap<String, (u64, u64)>>,
    timeline_step: Mutex<Option<transport::dto::TaskProgressWorkDto>>,
}

impl CapabilityProgress {
    pub fn new(job: &str, row: &SourceCapabilityRow, kind: ExecutableCapability) -> Self {
        Self {
            reporter: Mutex::new(TaskProgressReporter::for_capability(
                job,
                &row.case_id,
                &row.data_source_id,
                kind == ExecutableCapability::Artifacts,
            )),
            categories: Mutex::new(BTreeMap::new()),
            timeline_step: Mutex::new(None),
        }
    }

    pub fn analysis(&self, work: &CapabilityWork, progress: &AnalysisExtractionProgressDto) {
        let mut categories = self.categories.lock().unwrap();
        categories.insert(
            progress.category.clone(),
            (progress.processed_candidates, progress.total_candidates),
        );
        let done = categories.values().map(|(done, _)| *done).sum();
        let total = categories.values().map(|(_, total)| *total).sum();
        let mut reporter = self.reporter.lock().unwrap();
        if let Some(event) = reporter.observe(Stage::Analyze, done, Some(total), Unit::Items) {
            emit(work, event);
        }
    }

    pub fn timeline(
        &self,
        work: &CapabilityWork,
        step: transport::dto::TaskProgressWorkDto,
        done: u64,
        total: Option<u64>,
    ) {
        use transport::dto::TaskProgressWorkDto as Step;
        let stage = match step {
            Step::TimelineIdentity => Stage::Prepare,
            Step::TimelineWorkspace | Step::TimelineEvents => Stage::Index,
            Step::TimelineGraphPrepare | Step::TimelineGraph => Stage::Publish,
        };
        let mut previous = self.timeline_step.lock().unwrap();
        let mut reporter = self.reporter.lock().unwrap();
        if *previous != Some(step) {
            if stage == Stage::Index {
                emit(work, reporter.finish_stage(Stage::Prepare, false));
            }
            if stage == Stage::Publish {
                emit(work, reporter.finish_stage(Stage::Index, false));
            }
            emit(work, reporter.start_work(stage, Some(step)));
            *previous = Some(step);
        }
        if let Some(event) =
            reporter.advance_work(done, total, Unit::Items, stage != Stage::Publish)
        {
            emit(work, event);
        }
    }

    pub fn publish(&self, work: &CapabilityWork) {
        let mut reporter = self.reporter.lock().unwrap();
        let stage = if work.kind == ExecutableCapability::Artifacts {
            Stage::Analyze
        } else {
            Stage::Index
        };
        emit(work, reporter.finish_stage(stage, false));
        emit(work, reporter.start(Stage::Publish));
    }
}

fn emit(work: &CapabilityWork, event: transport::dto::TaskProgressDto) {
    if let Err(error) = events::emit(
        work.events.as_deref(),
        EventTopic::JobProgress,
        serde_json::to_value(event).expect("task progress serialization"),
    ) {
        tracing::warn!(%error, "Post-import task progress delivery failed");
    }
}

pub(super) fn observe(
    work: &CapabilityWork,
    cancel: &AtomicBool,
    operation: impl FnOnce() -> Result<&'static str, PostImportError>,
) -> Result<(), PostImportError> {
    {
        let mut reporter = work.progress.reporter.lock().unwrap();
        emit(work, reporter.start(Stage::Prepare));
        if work.kind == ExecutableCapability::Artifacts {
            emit(work, reporter.finish_stage(Stage::Prepare, false));
            emit(work, reporter.start(Stage::Analyze));
        }
    }
    std::thread::scope(|scope| {
        let (stop, receiver) = std::sync::mpsc::channel::<()>();
        let worker = scope.spawn(move || {
            while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                receiver.recv_timeout(Duration::from_secs(2))
            {
                let event = work
                    .progress
                    .reporter
                    .lock()
                    .unwrap()
                    .heartbeat(cancel.load(Ordering::Acquire));
                emit(work, event);
            }
        });
        let result = operation();
        drop(stop);
        worker.join().expect("post-import heartbeat worker");
        let state = match &result {
            Ok(state) => match *state {
                "ready" | "notApplicable" => State::Completed,
                "partial" | "deferred" => State::Partial,
                "cancelled" => State::Cancelled,
                _ => State::Failed,
            },
            Err(PostImportError::Cancelled) => State::Cancelled,
            Err(_) => State::Failed,
        };
        emit(work, work.progress.reporter.lock().unwrap().finish(state));
        result.map(|_| ())
    })
}
