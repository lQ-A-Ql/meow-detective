use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use persistence_sqlite::repositories::job_repo::JobRepo;

use crate::import_pipeline::options::JobOutcomeCounts;
use crate::import_pipeline::{emit, ImportJobOptions};
use crate::import_precheck;

pub(crate) struct ImportJobContext<'a> {
    pub conn: &'a rusqlite::Connection,
    pub source_conn: Option<&'a rusqlite::Connection>,
    pub case_id: &'a domain::CaseId,
    pub case_root: &'a std::path::Path,
    pub source_path: &'a str,
    pub job_id: &'a domain::JobId,
    pub options: ImportJobOptions<'a>,
    pub import_config: import_precheck::ImportSourceConfig,
    pub ds: Option<&'a domain::DataSource>,
    pub job_repo: JobRepo<'a>,
    pub counts: &'a mut JobOutcomeCounts,
    pub content_kind: ImportContentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportContentKind {
    Filesystem,
    CephBlueStoreMetadata,
}

impl<'a> ImportJobContext<'a> {
    pub fn cancel_requested(&self) -> bool {
        self.options.cancel_token.load(Ordering::Relaxed)
    }

    pub fn event_sink(&self) -> Option<&dyn emit::ImportEventSink> {
        self.options.event_sink
    }

    pub fn source_connection(&self) -> Result<&rusqlite::Connection, transport::CommandError> {
        self.source_conn.ok_or_else(|| {
            transport::CommandError::internal("source DB connection is not initialized")
        })
    }

    pub fn report_job_progress(
        &self,
        progress: u32,
        detail: &str,
    ) -> Result<(), transport::CommandError> {
        self.job_repo
            .update_progress(self.job_id, progress, detail)
            .map_err(transport::CommandError::from_service_error)?;
        emit::emit_job_progress(self.event_sink(), &self.job_id.0, progress, detail);
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct PhaseTelemetry {
    started: Instant,
}

impl PhaseTelemetry {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.elapsed().as_millis()
    }
}

impl Default for PhaseTelemetry {
    fn default() -> Self {
        Self::new()
    }
}
