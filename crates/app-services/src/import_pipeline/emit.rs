use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto,
};

/// UI/event bridge used by the import pipeline.
///
/// Implementations must be best-effort: event delivery failures should be
/// logged by the adapter and must not interrupt forensic import work.
pub trait ImportEventSink: Sync {
    fn job_progress(&self, job_id: &str, progress: u32, detail: &str);

    fn partition_progress(
        &self,
        job_id: &str,
        current_partition: &str,
        completed: u32,
        total: u32,
        partition_pct: u32,
    );

    fn timeline_updated(&self, event_count: u64);

    fn search_index_progress(&self, progress: u32, detail: &str);

    fn data_source_imported(&self, case_id: &str, data_source: &DataSourceSummaryDto, job_id: &str);

    fn import_phase_progress(&self, progress: &ImportPhaseProgressDto);

    fn import_partial_result(&self, result: &PartialResultDto);

    fn cache_index_status(&self, status: &IndexCacheStatusDto);

    fn job_cancellation(&self, cancellation: &JobCancellationDto);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopImportEventSink;

impl ImportEventSink for NoopImportEventSink {
    fn job_progress(&self, _job_id: &str, _progress: u32, _detail: &str) {}

    fn partition_progress(
        &self,
        _job_id: &str,
        _current_partition: &str,
        _completed: u32,
        _total: u32,
        _partition_pct: u32,
    ) {
    }

    fn timeline_updated(&self, _event_count: u64) {}

    fn search_index_progress(&self, _progress: u32, _detail: &str) {}

    fn data_source_imported(
        &self,
        _case_id: &str,
        _data_source: &DataSourceSummaryDto,
        _job_id: &str,
    ) {
    }

    fn import_phase_progress(&self, _progress: &ImportPhaseProgressDto) {}

    fn import_partial_result(&self, _result: &PartialResultDto) {}

    fn cache_index_status(&self, _status: &IndexCacheStatusDto) {}

    fn job_cancellation(&self, _cancellation: &JobCancellationDto) {}
}

pub(crate) fn emit_job_progress(
    sink: Option<&dyn ImportEventSink>,
    job_id: &str,
    progress: u32,
    detail: &str,
) {
    if let Some(sink) = sink {
        sink.job_progress(job_id, progress, detail);
    }
}

pub(crate) fn emit_partition_progress(
    sink: Option<&dyn ImportEventSink>,
    job_id: &str,
    current_partition: &str,
    completed: u32,
    total: u32,
    partition_pct: u32,
) {
    if let Some(sink) = sink {
        sink.partition_progress(job_id, current_partition, completed, total, partition_pct);
    }
}

pub(crate) fn emit_timeline_updated(sink: Option<&dyn ImportEventSink>, event_count: u64) {
    if let Some(sink) = sink {
        sink.timeline_updated(event_count);
    }
}

pub(crate) fn emit_search_index_progress(
    sink: Option<&dyn ImportEventSink>,
    progress: u32,
    detail: &str,
) {
    if let Some(sink) = sink {
        sink.search_index_progress(progress, detail);
    }
}

pub(crate) fn emit_data_source_imported(
    sink: Option<&dyn ImportEventSink>,
    case_id: &str,
    data_source: &DataSourceSummaryDto,
    job_id: &str,
) {
    if let Some(sink) = sink {
        sink.data_source_imported(case_id, data_source, job_id);
    }
}

pub(crate) fn emit_import_phase_progress(
    sink: Option<&dyn ImportEventSink>,
    progress: &ImportPhaseProgressDto,
) {
    if let Some(sink) = sink {
        sink.import_phase_progress(progress);
    }
}

pub(crate) fn emit_import_partial_result(
    sink: Option<&dyn ImportEventSink>,
    result: &PartialResultDto,
) {
    if let Some(sink) = sink {
        sink.import_partial_result(result);
    }
}

pub(crate) fn emit_cache_index_status(
    sink: Option<&dyn ImportEventSink>,
    status: &IndexCacheStatusDto,
) {
    if let Some(sink) = sink {
        sink.cache_index_status(status);
    }
}

pub(crate) fn emit_job_cancellation(
    sink: Option<&dyn ImportEventSink>,
    cancellation: &JobCancellationDto,
) {
    if let Some(sink) = sink {
        sink.job_cancellation(cancellation);
    }
}
