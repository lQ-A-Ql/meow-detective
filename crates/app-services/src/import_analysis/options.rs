use super::budget::ContentBudget;
use super::tier::TierStateMachine;
use domain::DataSourceId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub type AnalysisProgressCallback<'a> = &'a dyn Fn(u32, &str);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportAnalysisMode {
    #[default]
    MetadataOnly,
    BudgetedContent,
    FullContent,
}

impl ImportAnalysisMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadataOnly",
            Self::BudgetedContent => "budgetedContent",
            Self::FullContent => "fullContent",
        }
    }

    pub fn allows_content(self) -> bool {
        !matches!(self, Self::MetadataOnly)
    }
}

#[derive(Debug, Clone)]
pub struct ImportAnalysisOptions {
    pub case_root: PathBuf,
    pub db_path: PathBuf,
    pub case_id: String,
    pub data_source_id: DataSourceId,
    pub index_dir: PathBuf,
    pub max_analysis_workers: Option<usize>,
    pub cancel_token: Arc<AtomicBool>,
    pub enable_timeline_projection: bool,
    pub enable_content_extraction: bool,
    pub enable_text_indexing: bool,
    pub analysis_mode: ImportAnalysisMode,
    pub content_budget: ContentBudget,
    pub memory_soft_limit_mb: u64,
    pub memory_hard_limit_mb: u64,
    /// See [`PostImportPipelineOptions::tier_state`].
    #[allow(clippy::type_complexity)]
    pub tier_state: Arc<Mutex<TierStateMachine>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportAnalysisStats {
    pub processed_count: u64,
    pub artifact_count: u64,
    pub timeline_count: u64,
    pub indexed_count: u64,
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
    pub worker_ids: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobOutcomeCounts {
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

impl JobOutcomeCounts {
    pub fn add_warnings(&mut self, count: usize) {
        self.warning_count = self.warning_count.saturating_add(count as u32);
    }

    pub fn add_skipped(&mut self, count: u32) {
        self.skipped_count = self.skipped_count.saturating_add(count);
    }

    pub fn add_failed(&mut self, count: u32) {
        self.failed_count = self.failed_count.saturating_add(count);
    }

    pub fn is_partial(&self) -> bool {
        self.warning_count > 0 || self.skipped_count > 0 || self.failed_count > 0
    }
}

#[derive(Debug, Clone)]
pub struct PostImportPipelineOptions {
    pub case_root: PathBuf,
    pub db_path: PathBuf,
    pub case_id: String,
    pub data_source_id: DataSourceId,
    pub index_dir: PathBuf,
    pub max_analysis_workers: Option<usize>,
    pub cancel_token: Arc<AtomicBool>,
    pub enable_timeline_projection: bool,
    pub enable_content_extraction: bool,
    pub enable_text_indexing: bool,
    pub analysis_mode: ImportAnalysisMode,
    /// Optional tier state machine for tracking post-import pipeline progress.
    /// When provided, the pipeline advances through Catalog → ExtractArtifacts →
    /// CorrelateAndIndex and the caller can inspect partial results after each tier.
    #[allow(clippy::type_complexity)]
    pub tier_state: Arc<Mutex<TierStateMachine>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostImportPipelineError {
    pub message: String,
    pub counts: JobOutcomeCounts,
}
