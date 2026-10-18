//! Explicit, source-scoped PNG carving. Ordinary imports never call this use case.
mod access;
mod persistence;
mod query;
mod range;
mod run;
pub(crate) use access::{source_revision, validate_request};
pub use query::search_carved_artifacts;
pub use range::{export_carved_artifact, read_carved_artifact_range};
pub use run::run_png_carving;
