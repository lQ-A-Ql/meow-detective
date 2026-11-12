//! Facade over `analysis-core`; keeps `crate::analysis_service::*` paths
//! stable.
//!
//! The analysis capability (candidate discovery, extraction orchestration,
//! classification, summaries, system information) lives in `analysis-core`.
//! Symbols that were `pub(crate)` within this crate before the move stay
//! crate-visible here, while everything else keeps its historical public path.
pub use ::analysis_core::analysis_service::*;

// The source evidence scan drives this crate's artifact extractors and parser
// plugin host, so its implementation lives in `artifact_service`; re-exported
// here so the historical `analysis_service::run_source_evidence_scan*` paths
// stay stable for the command layer and import pipeline.
pub use crate::artifact_service::{run_source_evidence_scan, run_source_evidence_scan_with_cancel};
