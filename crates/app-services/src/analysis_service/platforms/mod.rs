mod analyzer;
mod evidence;
mod linux;
mod source;
mod windows;

pub use analyzer::validate_analysis_categories;
pub(crate) use analyzer::{analyzer_for, PlatformAnalyzer};
pub(crate) use evidence::evidence_summary_category_allowed;
pub use evidence::select_evidence_scan_categories;
pub use source::{resolve_data_source_platform, validate_data_source_analysis_categories};
