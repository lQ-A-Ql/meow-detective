use std::path::Path;

use crate::datasource_service::ImageFilesystemCandidate;
use crate::import_pipeline::profile::{Counters, PhaseRecorder};
use crate::import_pipeline::ImportProfiler;
use crate::parallel_enum::PartitionWork;
use domain::DataSourceKind;
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

pub(crate) fn build_partition_work_profiled(
    source_path: &Path,
    source_kind: &DataSourceKind,
    partition: &crate::staging::PartitionEntry,
    candidates: &[ImageFilesystemCandidate],
    profiler: Option<ImportProfiler>,
) -> Option<PartitionWork> {
    let mut profile = PhaseRecorder::new(profiler, Some(partition.index));
    let started = profile.start();
    let work = super::build_partition_work(
        source_path,
        source_kind,
        partition.index,
        &partition.name,
        &partition.fs_kind,
        candidates,
    );
    profile.record(
        Phase::ReaderBuild,
        started,
        if work.is_some() {
            State::Completed
        } else {
            State::Failed
        },
    );
    profile.count(
        Phase::ReaderBuild,
        Counters {
            builds: u64::from(work.is_some()),
            ..Default::default()
        },
    );
    work
}
