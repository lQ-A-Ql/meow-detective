//! Import pre-check and planning.
//!
//! Analyzes data sources before import to generate optimal import plans.

mod check;
mod error;
mod prepare;

pub use check::{pre_import_check, PreCheckResult};
pub use error::ImportSourceConfigError;
pub use prepare::{
    prepare_import_source_config, prepare_import_source_config_from_path,
    prepare_import_source_config_from_path_with_kind, prepare_import_source_config_with_kind,
    ImportClusterMemberConfig, ImportSourceConfig, ImportSourceMode,
};

#[cfg(test)]
#[path = "../tests/unit/import_precheck.rs"]
mod tests;
