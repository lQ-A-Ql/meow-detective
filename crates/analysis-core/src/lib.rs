//! Host-platform analysis capability layer.
//!
//! This crate owns candidate discovery, artifact extraction orchestration,
//! platform-aware classification and summarisation, the registry browser
//! service, and the Linux identity evidence facts assembled during import.
//! It sits above `evidence-access`, `source-kernel`, and `analysis-plugins`
//! and must never depend on `app-services` or Tauri.

pub mod analysis_service;
pub mod linux_evidence_facts;
pub mod registry_browser_service;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
