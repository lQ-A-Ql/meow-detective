//! Catalog indexing and projection for file catalog.
//!
//! Provides in-memory projections for efficient file catalog queries:
//! - `ExtensionProjection`: group files by extension
//! - `PathPrefixProjection`: group files by path prefix
//! - `CatalogIndex`: main index with materialized projections

pub mod indexing;
pub mod projection;

pub use indexing::CatalogIndex;
pub use projection::{ExtensionProjection, PathPrefixProjection};
