//! Source build database facade.
//!
//! The build-database protocol (`source.db.build.<attempt>` staging, seal,
//! verify, atomic publish, discard) is owned by `persistence-source`; this
//! module keeps the existing `crate::source_db::build::*` paths.

pub use persistence_source::publication::build::*;
