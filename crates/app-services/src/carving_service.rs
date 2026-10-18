//! Explicit, source-scoped PNG carving. Ordinary imports never call this use case.
mod access;
mod persistence;
mod query;
mod range;
mod run;
mod run_zip;
pub(crate) use access::{
    source_revision, source_revision_for, validate_request, validate_zip_request,
};
pub use query::search_carved_artifacts;
pub use range::{export_carved_artifact, read_carved_artifact_range};
pub use run::run_png_carving;
pub use run_zip::run_zip_carving;
