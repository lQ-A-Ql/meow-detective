//! # Ingest Pipeline (DEPRECATED)
//!
//! This crate defines the IngestPipeline trait and related types for evidence ingestion.
//!
//! **DEPRECATED**: The production ingestion path lives in
//! pps/desktop/src-tauri/src/commands/import/pipeline.rs.
//! This crate is retained for reference but is not used by any production code.
//! Scheduled for removal in a future cleanup pass after confirming no external
//! consumers depend on the trait shapes defined here.
//!
//! Ingestion pipeline orchestration.
//!
//! Defines the trait-based pipeline for importing data sources into a case:
//! - Source classification (image vs logical directory)
//! - Partition enumeration and filesystem traversal
//! - File entry extraction and persistence
//! - Post-import pipeline (timeline projection, artifact extraction, text indexing)

pub mod config;
pub mod graph_writer;
pub mod pipeline;
pub mod sink;
pub mod stats;

pub use config::IngestConfig;
pub use graph_writer::{GraphWriter, SqliteGraphWriter};
pub use pipeline::IngestPipeline;
pub use sink::IngestSink;
pub use stats::IngestStats;

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
