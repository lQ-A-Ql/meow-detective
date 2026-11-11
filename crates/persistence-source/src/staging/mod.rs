//! Staging merge semantics.

pub mod analysis;
pub mod catalog;

/// Compatibility alias for callers that used the `analysis_staging` module name.
pub mod analysis_staging {
    pub use super::analysis::*;
}

pub use catalog::merge_staging_catalog;
