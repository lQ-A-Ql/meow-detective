use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::AppHandle;

use crate::import_analysis;

/// Aggregated outcome counts for an import job.
#[derive(Debug, Clone, Default)]
pub struct JobOutcomeCounts {
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

/// Options controlling import job execution.
#[derive(Clone, Copy)]
pub struct ImportJobOptions<'a> {
    pub app: Option<&'a AppHandle>,
    pub cancel_token: &'a Arc<AtomicBool>,
    pub max_import_workers: Option<usize>,
    pub max_analysis_workers: Option<usize>,
    pub analysis_mode: import_analysis::ImportAnalysisMode,
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
