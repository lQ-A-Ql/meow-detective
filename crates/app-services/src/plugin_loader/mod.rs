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
pub use extractor::PluginExtractor;
pub use library::load_plugins_from_dirs;
pub use settings::{load_plugin_settings, PluginSettings};

use artifacts_core::ArtifactExtractor;

/// Discover, validate and load all parser plugins for the current process.
///
/// Honors `plugins.enabled` / `plugins.dir`. Every per-plugin failure is
/// logged and skipped, never fatal. Returns an empty vector when plugins are
/// disabled, no plugin directory exists, or the host is not Windows.
pub fn load_all() -> Vec<Box<dyn ArtifactExtractor>> {
    let settings = settings::load_plugin_settings();
    if !settings.enabled {
        tracing::info!("parser plugins disabled via plugins.enabled=false");
        return Vec::new();
    }
    let root = settings.dir.or_else(directory::default_plugins_root);
    let Some(root) = root else {
        return Vec::new();
    };
    let dirs = directory::plugin_search_dirs(&root);
    if dirs.is_empty() {
        return Vec::new();
    }
    library::load_plugins_from_dirs(&dirs)
        .into_iter()
        .map(|extractor| Box::new(extractor) as Box<dyn ArtifactExtractor>)
        .collect()
}
