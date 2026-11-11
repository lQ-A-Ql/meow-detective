//! Case-managed path safety facade.
//!
//! The guards are owned by `persistence-source` so that `source.db` opening and
//! publication validate paths with a single implementation; this module keeps
//! the existing `crate::source_db::path_safety::*` paths.

pub use persistence_source::control::path_safety::*;
