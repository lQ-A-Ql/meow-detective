//! Import pipeline module.
//!
//! Handles data source import logic including:
//! - File reader factory for different source types
//! - Image (E01/RAW) import with partition detection
//! - Logical directory import
//! - Post-import pipeline (timeline projection, artifact extraction, text indexing)

pub mod pipeline;
