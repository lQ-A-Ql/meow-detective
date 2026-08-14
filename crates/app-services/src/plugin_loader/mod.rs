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
///
/// The discovery pass runs once per process: DLLs are loaded and their
/// identity strings leaked exactly once (see `library::SharedPlugin`), and
/// each call builds fresh extractors over the shared handles. `plugins.*`
/// settings are therefore read on the first call only.
pub fn load_all_report() -> PluginLoadReport {
    let shared = shared_plugins();
    PluginLoadReport {
        plugins: shared
            .plugins
            .iter()
            .map(|plugin| PluginExtractor::shared(std::sync::Arc::clone(plugin)))
            .collect(),
        rejections: shared.rejections.clone(),
    }
}

/// The process-level discovery result (loaded once, shared by every
/// registry build).
#[cfg(windows)]
fn shared_plugins() -> &'static library::SharedPluginLoad {
    static CACHE: std::sync::OnceLock<library::SharedPluginLoad> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let settings = settings::load_plugin_settings();
        if !settings.enabled {
            tracing::info!("parser plugins disabled via plugins.enabled=false");
            return library::SharedPluginLoad::default();
        }
        let root = settings.dir.or_else(directory::default_plugins_root);
        let Some(root) = root else {
            return library::SharedPluginLoad::default();
        };
        let dirs = directory::plugin_search_dirs(&root);
        if dirs.is_empty() {
            return library::SharedPluginLoad::default();
        }
        library::load_shared_plugins(&dirs)
    })
}

/// Non-Windows hosts never load plugins.
#[cfg(not(windows))]
fn shared_plugins() -> &'static library::SharedPluginLoad {
    static EMPTY: library::SharedPluginLoad = library::SharedPluginLoad {
        plugins: Vec::new(),
        rejections: Vec::new(),
    };
    &EMPTY
}
