use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

use super::benchmark::{Counters, ImportProfiler, Measurement};

/// Accumulate chunk timings locally; publish once per partition, never per row.
pub(crate) struct PhaseRecorder {
    profiler: Option<ImportProfiler>,
    partition: Option<usize>,
    measurements: BTreeMap<Phase, Measurement>,
}

impl PhaseRecorder {
    pub(crate) fn new(profiler: Option<ImportProfiler>, partition: Option<usize>) -> Self {
        Self {
            profiler,
            partition,
            measurements: BTreeMap::new(),
        }
    }

    pub(crate) fn measure<T, E>(
        &mut self,
        phase: Phase,
        operation: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let started = self.start();
        let result = operation();
        self.record(
            phase,
            started,
            if result.is_ok() {
                State::Completed
            } else {
                State::Failed
            },
        );
        result
    }

    pub(crate) fn start(&self) -> Option<Instant> {
        self.profiler.as_ref().map(|_| Instant::now())
    }

    pub(crate) fn record(&mut self, phase: Phase, started: Option<Instant>, state: State) {
        if let Some(started) = started {
            self.record_duration(phase, Some(started.elapsed()), state);
        }
    }

    pub(crate) fn record_duration(
        &mut self,
        phase: Phase,
        duration: Option<Duration>,
        state: State,
    ) {
        if self.profiler.is_none() {
            return;
        }
        let sample = Measurement {
            state,
            duration,
            counters: Counters::default(),
            failures: u64::from(state == State::Failed),
        };
        match self.measurements.get_mut(&phase) {
            Some(existing) => existing.merge(&sample),
            None => {
                self.measurements.insert(phase, sample);
            }
        }
    }

    pub(crate) fn count(&mut self, phase: Phase, counters: Counters) {
        if let Some(sample) = self.measurements.get_mut(&phase) {
            sample.counters.add(&counters);
        }
    }

    pub(crate) fn cancel(&mut self, phase: Phase) {
        if let Some(sample) = self.measurements.get_mut(&phase) {
            sample.state = State::Cancelled;
            sample.failures = 0;
        }
    }
}

impl Drop for PhaseRecorder {
    fn drop(&mut self) {
        if let Some(profiler) = &self.profiler {
            profiler.publish(self.partition, std::mem::take(&mut self.measurements));
        }
    }
}
