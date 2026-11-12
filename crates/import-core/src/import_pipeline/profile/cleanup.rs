//! Observe the workflow's existing local destruction order without changing it.
use super::{benchmark::Counters, ImportProfiler};
use std::time::Instant;
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

pub(crate) struct CleanupRecorder {
    profiler: Option<ImportProfiler>,
    started: Option<Instant>,
}

impl CleanupRecorder {
    pub(crate) fn new(profiler: Option<ImportProfiler>) -> Self {
        Self {
            profiler,
            started: None,
        }
    }

    pub(crate) fn start(&mut self) {
        self.started = self.profiler.as_ref().map(|_| Instant::now());
    }
}

impl Drop for CleanupRecorder {
    fn drop(&mut self) {
        if let (Some(profiler), Some(started)) = (&self.profiler, self.started) {
            profiler.record(
                None,
                Phase::Cleanup,
                Some(started.elapsed()),
                State::Completed,
                Counters::default(),
            );
        }
    }
}
