//! Evidence candidate discovery facade.
//!
//! Platform-specific rules live beside one another while this module preserves
//! the original `analysis_service::candidates::*` API surface.

mod common;
mod linux;
mod plugin;
mod summary;
mod windows;

pub use common::{
    collect_file_entries, discover_evidence_candidates, evidence_candidates_for_categories,
    EvidenceCandidate, EvidenceCategoryDef,
};
pub(crate) use common::{
    evidence_candidates_for_categories_with_cancel, find_candidate_by_path_suffix,
    normalize_evidence_path, row_to_file_entry_for_analysis,
};
pub use plugin::discover_plugin_candidates;
pub use summary::get_evidence_classification_summary;
pub(crate) use windows::is_browser_history_path;

use common::{EMAIL_CATEGORY_DEF, FILE_TYPE_INVENTORY_CATEGORY_DEF};
use linux::LINUX_ARTIFACTS_CATEGORY_DEF;
use windows::{
    BROWSER_HISTORY_CATEGORY_DEF, EVENT_LOGS_CATEGORY_DEF, PROGRAM_EXECUTION_CATEGORY_DEF,
    RECYCLE_BIN_CATEGORY_DEF, REGISTRY_CATEGORY_DEF, RESOURCE_USAGE_CATEGORY_DEF,
    SYSTEM_INFORMATION_CATEGORY_DEF, THUMBNAILS_CATEGORY_DEF, USER_ACTIVITY_CATEGORY_DEF,
};

pub(crate) const UNSUPPORTED_MACOS_CATEGORY: &str = "MacArtifacts";

const EVIDENCE_CATEGORY_DEFS: &[EvidenceCategoryDef] = &[
    SYSTEM_INFORMATION_CATEGORY_DEF,
    REGISTRY_CATEGORY_DEF,
    EVENT_LOGS_CATEGORY_DEF,
    PROGRAM_EXECUTION_CATEGORY_DEF,
    USER_ACTIVITY_CATEGORY_DEF,
    RECYCLE_BIN_CATEGORY_DEF,
    THUMBNAILS_CATEGORY_DEF,
    RESOURCE_USAGE_CATEGORY_DEF,
    BROWSER_HISTORY_CATEGORY_DEF,
    EMAIL_CATEGORY_DEF,
    FILE_TYPE_INVENTORY_CATEGORY_DEF,
    LINUX_ARTIFACTS_CATEGORY_DEF,
];

pub fn evidence_category_defs() -> &'static [EvidenceCategoryDef] {
    EVIDENCE_CATEGORY_DEFS
}
