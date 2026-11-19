use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::import_analysis;
use crate::import_pipeline::emit::ImportEventSink;

/// Aggregated outcome counts for an import job.
#[derive(Debug, Clone, Default)]
pub struct JobOutcomeCounts {
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

/// Where post-import capability work runs after catalog publication.
///
/// `Inline` keeps the legacy behavior: timeline materialization and artifact
/// extraction run inside the import job. `Detached` ends the job after catalog
/// publication and queues timeline/artifacts for the desktop post-import
/// scheduler. The mode rides on `ImportJobContext`; `ImportJobOptions` stays a
/// stable struct literal consumed by 20+ tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PostImportMode {
    /// Timeline materialization and artifact extraction run inside the job.
    #[default]
    Inline,
    /// The job ends after catalog publication; post-import work is queued.
    Detached,
}

/// Options controlling import job execution.
#[derive(Clone, Copy)]
pub struct ImportJobOptions<'a> {
    pub projection_policy: timeline::TimelineProjectionPolicy,
    pub event_sink: Option<&'a dyn ImportEventSink>,
    pub cancel_token: &'a Arc<AtomicBool>,
    pub max_import_workers: Option<usize>,
    pub max_analysis_workers: Option<usize>,
    pub analysis_mode: import_analysis::ImportAnalysisMode,
    /// Cluster imports can publish their Catalog before Linux artifact
    /// extraction. The desktop coordinator then runs that phase as a
    /// separate background task.
    pub defer_linux_artifacts: bool,
}

impl JobOutcomeCounts {
    pub(crate) fn add_warnings(&mut self, count: usize) {
        self.warning_count = self.warning_count.saturating_add(count as u32);
    }

    pub(crate) fn add_failed(&mut self, count: u32) {
        self.failed_count = self.failed_count.saturating_add(count);
    }

    pub(crate) fn is_partial(&self) -> bool {
        self.warning_count > 0 || self.skipped_count > 0 || self.failed_count > 0
    }
}

impl From<import_analysis::JobOutcomeCounts> for JobOutcomeCounts {
    fn from(counts: import_analysis::JobOutcomeCounts) -> Self {
        Self {
            warning_count: counts.warning_count,
            skipped_count: counts.skipped_count,
            failed_count: counts.failed_count,
        }
    }
}
