//! Shared types for the import pipeline execution.
//!
//! These types keep the orchestration layer (`execute.rs`) and the per-phase
//! implementation (`phases.rs`) decoupled while still passing the exact same
//! state that used to live as local variables inside the single monolithic
//! `execute_import_job_with_counts` function.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use persistence_sqlite::repositories::job_repo::JobRepo;

use crate::import_pipeline::options::JobOutcomeCounts;
use crate::import_pipeline::ImportJobOptions;
use crate::import_precheck;

/// Mutable state shared across the import phases.
///
/// This struct deliberately mirrors the local variables that the original
/// monolithic function kept on its stack. Putting them in one place makes the
/// data flow explicit and prevents each phase signature from growing to a dozen
/// parameters.
pub(crate) struct ImportJobContext<'a> {
    pub conn: &'a rusqlite::Connection,
    pub case_id: &'a domain::CaseId,
    pub case_root: &'a std::path::Path,
    pub source_path: &'a str,
    pub job_id: &'a domain::JobId,
    pub options: ImportJobOptions<'a>,
    pub import_config: import_precheck::ImportSourceConfig,
    pub ds: Option<&'a domain::DataSource>,
    pub job_repo: JobRepo<'a>,
    pub counts: &'a mut JobOutcomeCounts,
}

impl<'a> ImportJobContext<'a> {
    /// Convenience accessor for the cancellation flag.
    pub fn cancel_requested(&self) -> bool {
        self.options.cancel_token.load(Ordering::Relaxed)
    }

    /// Convenience accessor for the optional Tauri app handle.
    pub fn app(&self) -> Option<&tauri::AppHandle> {
        self.options.app
    }

    /// Persist job progress to the repository and emit it to the frontend if an
    /// app handle is available.
    pub fn report_job_progress(
        &self,
        progress: u32,
        detail: &str,
    ) -> Result<(), transport::CommandError> {
        self.job_repo
            .update_progress(self.job_id, progress, detail)
            .map_err(transport::CommandError::from_service_error)?;
        if let Some(app) = self.app() {
            crate::import_pipeline::emit::emit_job_progress(app, &self.job_id.0, progress, detail);
        }
        Ok(())
    }
}

/// Lightweight per-phase timer used for telemetry strings.
///
/// Phase functions create a `PhaseTelemetry` at their entry point and read its
/// elapsed time when building profile detail strings.
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
