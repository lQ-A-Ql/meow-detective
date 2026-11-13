//! Facade over `analysis-core`; keeps `crate::analysis_service::*` paths
//! stable.
//!
//! The analysis capability (candidate discovery, extraction orchestration,
//! classification, summaries, system information) lives in `analysis-core`.
//! Symbols that were `pub(crate)` within this crate before the move stay
//! crate-visible here, while everything else keeps its historical public path.
pub use ::analysis_core::analysis_service::*;
