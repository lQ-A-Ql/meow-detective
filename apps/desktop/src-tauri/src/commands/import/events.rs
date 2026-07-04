//! Desktop adapter for import pipeline events.

use app_services::import_pipeline::ImportEventSink;
use tauri::AppHandle;
use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto,
};

use crate::events::event_bridge;

pub(crate) struct TauriImportEventSink<'a> {
    app: &'a AppHandle,
}

impl<'a> TauriImportEventSink<'a> {
    pub(crate) fn new(app: &'a AppHandle) -> Self {
        Self { app }
    }
}

impl ImportEventSink for TauriImportEventSink<'_> {
    fn job_progress(&self, job_id: &str, progress: u32, detail: &str) {
        event_bridge::emit_job_progress(self.app, job_id, progress, detail);
    }

    fn partition_progress(
        &self,
        job_id: &str,
        current_partition: &str,
        completed: u32,
        total: u32,
        partition_pct: u32,
    ) {
        event_bridge::emit_partition_progress(
            self.app,
            job_id,
            current_partition,
            completed,
            total,
            partition_pct,
        );
    }

    fn timeline_updated(&self, event_count: u64) {
        event_bridge::emit_timeline_updated(self.app, event_count);
    }

    fn search_index_progress(&self, progress: u32, detail: &str) {
        event_bridge::emit_search_index_progress(self.app, progress, detail);
    }

    fn data_source_imported(
        &self,
        case_id: &str,
        data_source: &DataSourceSummaryDto,
        job_id: &str,
    ) {
        event_bridge::emit_data_source_imported(self.app, case_id, data_source, job_id);
    }

    fn import_phase_progress(&self, progress: &ImportPhaseProgressDto) {
        event_bridge::emit_import_phase_progress(self.app, progress);
    }

    fn import_partial_result(&self, result: &PartialResultDto) {
        event_bridge::emit_import_partial_result(self.app, result);
    }

    fn cache_index_status(&self, status: &IndexCacheStatusDto) {
        event_bridge::emit_cache_index_status(self.app, status);
    }

    fn job_cancellation(&self, cancellation: &JobCancellationDto) {
        event_bridge::emit_job_cancellation(self.app, cancellation);
    }
}
