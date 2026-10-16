mod benchmark;
mod parsing;
pub(crate) mod progress;
mod recorder;
pub(crate) mod results;
mod storage;

pub(crate) use benchmark::Counters;
pub use benchmark::ImportProfiler;
pub(crate) use parsing::{bytes_to_mb, elapsed_ms, mb_per_sec, rows_per_sec};
pub(crate) use progress::{emit_import_profile_progress, emit_phase_profile};
pub(crate) use recorder::PhaseRecorder;
pub(crate) use results::post_import_counts_from_message;
pub(crate) use storage::sample_wal_size;

pub(crate) fn measure_optional<T, E>(
    profiler: Option<&ImportProfiler>,
    phase: transport::dto::import_benchmark::ImportBenchmarkPhaseDto,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    match profiler {
        Some(profiler) => profiler.measure(phase, operation),
        None => operation(),
    }
}
