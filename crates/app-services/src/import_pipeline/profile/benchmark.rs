use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkProcessUsageDto, ImportBenchmarkReportDto,
    ImportBenchmarkSampleDto, ImportBenchmarkStateDto as State,
};

type SampleKey = (Option<usize>, Phase);

/// Job-local profiling collector. Normal imports need not allocate a collector.
#[derive(Debug, Clone, Default)]
pub struct ImportProfiler {
    samples: Arc<Mutex<BTreeMap<SampleKey, Measurement>>>,
    process_usage: Arc<Mutex<ProcessUsageState>>,
}

#[derive(Debug, Default)]
struct ProcessUsageState {
    windows: u64,
    usage: Option<ImportBenchmarkProcessUsageDto>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Counters {
    pub scanned: u64,
    pub parsed: u64,
    pub staged: u64,
    pub bytes: u64,
    pub opens: u64,
    pub builds: u64,
    pub buffered_batches: Option<u64>,
    pub buffered_records: Option<u64>,
    pub updated_main_rows: Option<u64>,
    pub sampled_wal_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct Measurement {
    pub state: State,
    pub duration: Option<Duration>,
    pub counters: Counters,
    pub failures: u64,
}

impl ImportProfiler {
    pub(crate) fn record_process_usage(&self, usage: ImportBenchmarkProcessUsageDto) {
        let mut state = self
            .process_usage
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.windows = state.windows.saturating_add(1);
        // A reused profiler cannot attribute multiple process windows to one workflow.
        state.usage = (state.windows == 1).then_some(usage);
    }

    pub(crate) fn record(
        &self,
        partition: Option<usize>,
        phase: Phase,
        duration: Option<Duration>,
        state: State,
        counters: Counters,
    ) {
        let measurement = Measurement {
            state,
            duration,
            counters,
            failures: u64::from(state == State::Failed),
        };
        self.publish(partition, BTreeMap::from([(phase, measurement)]));
    }

    pub(crate) fn measure<T, E>(
        &self,
        phase: Phase,
        operation: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let started = Instant::now();
        let result = operation();
        self.record(
            None,
            phase,
            Some(started.elapsed()),
            if result.is_ok() {
                State::Completed
            } else {
                State::Failed
            },
            Counters::default(),
        );
        result
    }

    pub(super) fn publish(
        &self,
        partition: Option<usize>,
        measurements: BTreeMap<Phase, Measurement>,
    ) {
        // Profiling must never abort evidence processing if a collector is poisoned.
        let mut samples = self
            .samples
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for (phase, measurement) in measurements {
            match samples.get_mut(&(partition, phase)) {
                Some(existing) => existing.merge(&measurement),
                None => {
                    samples.insert((partition, phase), measurement);
                }
            }
        }
    }

    pub fn report(&self) -> ImportBenchmarkReportDto {
        let samples = self
            .samples
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut aggregates: BTreeMap<Phase, Measurement> = BTreeMap::new();
        for ((partition, phase), measurement) in samples.iter().filter(|((p, _), _)| p.is_some()) {
            let _ = partition;
            match aggregates.get_mut(phase) {
                Some(existing) => existing.merge(measurement),
                None => {
                    aggregates.insert(*phase, measurement.clone());
                }
            }
        }
        ImportBenchmarkReportDto {
            schema_version: 1,
            samples: samples
                .iter()
                .map(|((partition, phase), sample)| sample.dto(*partition, *phase))
                .collect(),
            partition_aggregates: aggregates
                .iter()
                .map(|(phase, sample)| sample.dto(None, *phase))
                .collect(),
            rss_at_end_mb: crate::runtime_resources::current_rss_mb(),
            process_peak_rss_mb: crate::runtime_resources::peak_rss_mb(),
            process_usage: self
                .process_usage
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .usage
                .clone(),
            observation_limits: vec![
                transport::dto::import_benchmark::ImportBenchmarkObservationLimitDto::ProcessCountersSharedAcrossConcurrentJobs,
                transport::dto::import_benchmark::ImportBenchmarkObservationLimitDto::DevicePhysicalIoNotMeasured,
                transport::dto::import_benchmark::ImportBenchmarkObservationLimitDto::SqliteWalSizeSampledAtBatchBoundaries,
                transport::dto::import_benchmark::ImportBenchmarkObservationLimitDto::AnonymousSortFilePeakNotMeasured,
                transport::dto::import_benchmark::ImportBenchmarkObservationLimitDto::ChannelQueueDelayNotApplicableToWavePipeline,
            ],
        }
    }
}

impl Measurement {
    pub(super) fn merge(&mut self, other: &Self) {
        self.state = merge_state(self.state, other.state);
        self.duration = match (self.duration, other.duration) {
            (Some(a), Some(b)) => Some(a.saturating_add(b)),
            (a, b) => a.or(b),
        };
        self.counters.add(&other.counters);
        self.failures = self.failures.saturating_add(other.failures);
    }

    fn dto(&self, partition_index: Option<usize>, phase: Phase) -> ImportBenchmarkSampleDto {
        let seconds = self
            .duration
            .map(|duration| duration.as_secs_f64())
            .filter(|s| *s > 0.0);
        ImportBenchmarkSampleDto {
            phase,
            partition_index,
            state: self.state,
            inclusive: matches!(
                phase,
                Phase::Workflow
                    | Phase::Enumeration
                    | Phase::Analysis
                    | Phase::Timeline
                    | Phase::TimelineEvents
                    | Phase::TimelineGraph
                    | Phase::MftReadParseWall
                    | Phase::MftWriterWall
                    | Phase::MftPathResolve
                    | Phase::FallbackEnumeration
            ),
            elapsed_ns: self
                .duration
                .map(|d| d.as_nanos().min(u64::MAX as u128) as u64),
            scanned_records: self.counters.scanned,
            parsed_records: self.counters.parsed,
            staged_rows: self.counters.staged,
            logical_bytes_read: self.counters.bytes,
            physical_bytes_read: None,
            full_image_opens: self.counters.opens,
            filesystem_builds: self.counters.builds,
            failures: self.failures,
            max_buffered_batches: self.counters.buffered_batches,
            max_buffered_records: self.counters.buffered_records,
            main_table_updated_rows: self.counters.updated_main_rows,
            sampled_wal_max_bytes: self.counters.sampled_wal_bytes,
            records_per_sec: seconds
                .filter(|_| self.counters.scanned > 0)
                .map(|s| self.counters.scanned as f64 / s),
            logical_mb_per_sec: seconds
                .filter(|_| self.counters.bytes > 0)
                .map(|s| self.counters.bytes as f64 / (1024.0 * 1024.0) / s),
        }
    }
}

impl Counters {
    pub(super) fn add(&mut self, other: &Self) {
        self.scanned = self.scanned.saturating_add(other.scanned);
        self.parsed = self.parsed.saturating_add(other.parsed);
        self.staged = self.staged.saturating_add(other.staged);
        self.bytes = self.bytes.saturating_add(other.bytes);
        self.opens = self.opens.saturating_add(other.opens);
        self.builds = self.builds.saturating_add(other.builds);
        self.buffered_batches = self.buffered_batches.max(other.buffered_batches);
        self.buffered_records = self.buffered_records.max(other.buffered_records);
        self.sampled_wal_bytes = self.sampled_wal_bytes.max(other.sampled_wal_bytes);
        self.updated_main_rows = match (self.updated_main_rows, other.updated_main_rows) {
            (Some(a), Some(b)) => Some(a.saturating_add(b)),
            (a, b) => a.or(b),
        };
    }
}

pub(super) fn merge_state(a: State, b: State) -> State {
    if a == State::Failed || b == State::Failed {
        State::Failed
    } else if a == State::Cancelled || b == State::Cancelled {
        State::Cancelled
    } else if a == State::Completed || b == State::Completed {
        State::Completed
    } else {
        State::NotApplicable
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/import_pipeline/benchmark.rs"]
mod tests;
