//! # Catalog Indexing (DEPRECATED)
//!
//! This crate provides CatalogIndex, ExtensionProjection, and PathPrefixProjection
//! for file catalog indexing.
//!
//! **DEPRECATED**: This crate currently has no consumers in the production codebase.
//! The cataloging functionality has been absorbed into the import pipeline at
//! pps/desktop/src-tauri/src/commands/import/pipeline.rs.
//! Retained for reference; scheduled for removal in a future cleanup pass.
//!
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
