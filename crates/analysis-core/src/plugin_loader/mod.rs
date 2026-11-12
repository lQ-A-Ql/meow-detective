//! Host-side parser plugin loader (plugin system M2).
//!
//! Implements the host behavior described in
//! `docs/plugin-development-contract.md`: directory discovery (`directory`),
//! `plugins.*` settings (`settings`), safe DLL loading and the ABI handshake
//! (`library`), the `ArtifactExtractor` adapter with host-enforced provenance
//! (`extractor`), and the optional self-describing action channel (`action`).

mod action;
mod directory;
mod extractor;
mod library;
mod loader;
mod settings;

pub use directory::{default_plugins_root, plugin_search_dirs};
pub use extractor::{PluginExtractor, PluginModuleMeta};
pub use library::{
    load_plugins_from_dirs, load_plugins_from_dirs_reporting, PluginLoadReport, PluginRejection,
};
pub use loader::{load_all, load_all_report};
pub use settings::{load_plugin_settings, PluginSettings};
