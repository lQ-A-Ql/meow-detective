//! Host-side parser plugin integration.
//!
//! Owns plugin discovery and `plugins.*` settings, DLL loading and the ABI
//! handshake, the `ArtifactExtractor` adapter with host-enforced provenance,
//! and the optional self-describing action channel. It sits below
//! `analysis-core` and must never depend on `app-services` or Tauri.

pub mod plugin_action_service;
pub mod plugin_loader;
