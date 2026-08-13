//! Host-side parser plugin loader (plugin system M2).
//!
//! Implements the host behavior described in
//! `docs/plugin-abi-contract-design.md` §5: directory discovery (`directory`),
//! `plugins.*` settings (`settings`), safe DLL loading and the ABI handshake
//! (`library`), and the `ArtifactExtractor` adapter with host-enforced
//! provenance (`extractor`).

mod directory;
mod extractor;
mod library;
mod settings;

pub use directory::{default_plugins_root, plugin_search_dirs};
pub use extractor::{PluginExtractor, PluginModuleMeta};
pub use library::{
    load_plugins_from_dirs, load_plugins_from_dirs_reporting, PluginLoadReport, PluginRejection,
};
pub use settings::{load_plugin_settings, PluginSettings};

/// Discover, validate and load all parser plugins for the current process.
///
/// Honors `plugins.enabled` / `plugins.dir`. Every per-plugin failure is
/// logged and skipped, never fatal. Returns an empty vector when plugins are
/// disabled, no plugin directory exists, or the host is not Windows.
pub fn load_all() -> Vec<PluginExtractor> {
    load_all_report().plugins
}

/// [`load_all`] variant that also reports refused DLLs with their reasons so
/// callers with a case context can raise audit events.
pub fn load_all_report() -> PluginLoadReport {
    let settings = settings::load_plugin_settings();
    if !settings.enabled {
        tracing::info!("parser plugins disabled via plugins.enabled=false");
        return PluginLoadReport::default();
    }
    let root = settings.dir.or_else(directory::default_plugins_root);
    let Some(root) = root else {
        return PluginLoadReport::default();
    };
    let dirs = directory::plugin_search_dirs(&root);
    if dirs.is_empty() {
        return PluginLoadReport::default();
    }
    library::load_plugins_from_dirs_reporting(&dirs)
}
