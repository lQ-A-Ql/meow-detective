//! Plugin DLL loading, symbol resolution and ABI handshake (design doc §5.2–5.4).

use super::extractor::PluginExtractor;
use std::path::PathBuf;

#[cfg(windows)]
use super::extractor::{parse_families, parse_path_patterns, PathPattern, PluginMeta};
#[cfg(windows)]
use std::collections::HashSet;
#[cfg(windows)]
use std::path::Path;

/// A loaded plugin DLL plus its resolved entry points. Owns the library
/// mapping for as long as the extractor may call into it.
pub(crate) struct PluginLibrary {
    #[cfg(windows)]
    _library: libloading::os::windows::Library,
    extract: plugin_api::MeowPluginExtractFn,
    free_buffer: plugin_api::MeowPluginFreeBufferFn,
}

impl PluginLibrary {
    pub(crate) fn extract_fn(&self) -> plugin_api::MeowPluginExtractFn {
        self.extract
    }

    pub(crate) fn free_buffer_fn(&self) -> plugin_api::MeowPluginFreeBufferFn {
        self.free_buffer
    }
}

/// A fully loaded plugin shared across registry builds: the DLL is loaded
/// and its metadata leaked exactly once per process (see the module docs on
/// `load_all_report`'s process-level cache).
pub(crate) struct SharedPlugin {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) version: String,
    pub(crate) evidence_platform: plugin_api::MeowEvidencePlatform,
    pub(crate) primary_family: String,
    pub(crate) families: std::collections::HashSet<String>,
    pub(crate) declared_families: Vec<String>,
    pub(crate) patterns: Vec<PathPattern>,
    pub(crate) library: PluginLibrary,
}

impl SharedPlugin {
    /// Leak the identity strings once per process load; plugin DLLs are
    /// never unloaded, so this is bounded by the number of plugins.
    fn new(meta: PluginMeta, library: PluginLibrary) -> Self {
        let id: &'static str = Box::leak(meta.plugin_id.into_boxed_str());
        let display_name: &'static str = Box::leak(meta.display_name.into_boxed_str());
        let primary_family = meta
            .families
            .first()
            .cloned()
            .unwrap_or_else(|| id.to_string());
        Self {
            id,
            display_name,
            version: meta.version,
            evidence_platform: meta.evidence_platform,
            primary_family,
            families: meta.families.iter().cloned().collect(),
            declared_families: meta.families,
            patterns: meta.patterns,
            library,
        }
    }
}

/// A plugin DLL the host refused to load, with the reason.
#[derive(Debug, Clone)]
pub struct PluginRejection {
    pub path: PathBuf,
    pub reason: String,
}

/// Outcome of one plugin discovery pass: the plugins that loaded plus the
/// DLLs that were refused. Per-plugin failures are never fatal (§5.4).
#[derive(Default)]
pub struct PluginLoadReport {
    pub plugins: Vec<PluginExtractor>,
    pub rejections: Vec<PluginRejection>,
}

/// The cached form of a discovery pass: shared plugin handles plus cloned
/// rejections. `load_all_report` builds fresh extractors from this per call.
#[derive(Default)]
pub(crate) struct SharedPluginLoad {
    pub(crate) plugins: Vec<std::sync::Arc<SharedPlugin>>,
    pub(crate) rejections: Vec<PluginRejection>,
}

/// Load every valid plugin DLL found under `dirs` into shared handles.
#[cfg(windows)]
pub(crate) fn load_shared_plugins(dirs: &[PathBuf]) -> SharedPluginLoad {
    let mut seen_ids = HashSet::new();
    let mut result = SharedPluginLoad::default();
    for dir in dirs {
        for dll in enumerate_dlls(dir) {
            match try_load_plugin(&dll, &mut seen_ids) {
                Ok(plugin) => {
                    tracing::info!(
                        "loaded parser plugin '{}' v{} from {}",
                        plugin.id,
                        plugin.version,
                        dll.display()
                    );
                    result.plugins.push(std::sync::Arc::new(plugin));
                }
                Err(reason) => {
                    tracing::warn!("plugin {} refused: {}", dll.display(), reason);
                    result
                        .rejections
                        .push(PluginRejection { path: dll, reason });
                }
            }
        }
    }
    result
}

/// Load every valid plugin DLL found under `dirs`, reporting refusals.
#[cfg(windows)]
pub fn load_plugins_from_dirs_reporting(dirs: &[PathBuf]) -> PluginLoadReport {
    let shared = load_shared_plugins(dirs);
    PluginLoadReport {
        plugins: shared
            .plugins
            .iter()
            .map(|plugin| PluginExtractor::shared(std::sync::Arc::clone(plugin)))
            .collect(),
        rejections: shared.rejections,
    }
}

/// Non-Windows hosts load no plugins; the desktop host is Windows-first and
/// this stub keeps the cross-platform target graph compiling.
#[cfg(not(windows))]
pub fn load_plugins_from_dirs_reporting(_dirs: &[PathBuf]) -> PluginLoadReport {
    PluginLoadReport::default()
}

/// Load every valid plugin DLL found under `dirs`. Refused DLLs are logged
/// and skipped; they never abort the remaining plugins.
pub fn load_plugins_from_dirs(dirs: &[PathBuf]) -> Vec<PluginExtractor> {
    load_plugins_from_dirs_reporting(dirs).plugins
}

#[cfg(windows)]
fn enumerate_dlls(dir: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!("plugin directory {} unreadable: {}", dir.display(), error);
            return Vec::new();
        }
    };
    let mut dlls: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        })
        .collect();
    // Deterministic load order: first-seen wins the plugin_id dedup.
    dlls.sort();
    dlls
}

#[cfg(windows)]
fn try_load_plugin(path: &Path, seen_ids: &mut HashSet<String>) -> Result<SharedPlugin, String> {
    let library = open_library(path)?;
    let info = read_plugin_info(&library)?;
    let meta = read_plugin_meta(&info)?;
    let extract = resolve_symbol::<plugin_api::MeowPluginExtractFn>(
        &library,
        plugin_api::MEOW_PLUGIN_EXTRACT_SYMBOL,
    )?;
    let free_buffer = resolve_symbol::<plugin_api::MeowPluginFreeBufferFn>(
        &library,
        plugin_api::MEOW_PLUGIN_FREE_BUFFER_SYMBOL,
    )?;
    if !seen_ids.insert(meta.plugin_id.clone()) {
        return Err(format!("duplicate plugin id '{}'", meta.plugin_id));
    }
    Ok(SharedPlugin::new(
        meta,
        PluginLibrary {
            _library: library,
            extract,
            free_buffer,
        },
    ))
}

#[cfg(windows)]
fn open_library(path: &Path) -> Result<libloading::os::windows::Library, String> {
    let absolute =
        std::path::absolute(path).map_err(|error| format!("path not absolutized: {error}"))?;
    // SAFETY: the path is absolute and the flags confine dependent-DLL
    // resolution to System32 and the plugin's own directory, eliminating the
    // CWD/PATH search-order hijack surface. The handle is owned by the
    // returned Library and unloaded on drop.
    unsafe {
        libloading::os::windows::Library::load_with_flags(
            &absolute,
            libloading::os::windows::LOAD_LIBRARY_SEARCH_SYSTEM32
                | libloading::os::windows::LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
        )
    }
    .map_err(|error| format!("DLL load failed: {error}"))
}

#[cfg(windows)]
fn resolve_symbol<T: Copy>(
    library: &libloading::os::windows::Library,
    symbol: &[u8],
) -> Result<T, String> {
    // SAFETY: the symbol names and their fn pointer types are the plugin-api
    // contract constants; the pointer is copied out while the library mapping
    // stays alive inside PluginLibrary.
    unsafe { library.get::<T>(symbol) }
        .map(|symbol| *symbol)
        .map_err(|error| {
            let name = String::from_utf8_lossy(symbol);
            format!(
                "missing required export '{}': {}",
                name.trim_end_matches('\0'),
                error
            )
        })
}

#[cfg(windows)]
fn read_plugin_info(
    library: &libloading::os::windows::Library,
) -> Result<plugin_api::MeowPluginInfo, String> {
    let info_fn = resolve_symbol::<plugin_api::MeowPluginInfoFn>(
        library,
        plugin_api::MEOW_PLUGIN_INFO_SYMBOL,
    )?;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        // SAFETY: the contract guarantees meow_plugin_info is callable right
        // after load and returns plain data whose pointers live with the DLL.
        || unsafe { info_fn() },
    ));
    let info = outcome.map_err(|_| "panicked in meow_plugin_info".to_string())?;
    let expected = std::mem::size_of::<plugin_api::MeowPluginInfo>() as u32;
    if info.struct_size < expected {
        return Err(format!(
            "reports struct_size {} < expected {}",
            info.struct_size, expected
        ));
    }
    if info.abi_version != plugin_api::MEOW_PLUGIN_ABI_VERSION {
        return Err(format!(
            "ABI version {} != host {}",
            info.abi_version,
            plugin_api::MEOW_PLUGIN_ABI_VERSION
        ));
    }
    Ok(info)
}

#[cfg(windows)]
fn read_plugin_meta(info: &plugin_api::MeowPluginInfo) -> Result<PluginMeta, String> {
    // SAFETY: `info` passed the struct_size/abi_version handshake, so every
    // string field is either null or a NUL-terminated buffer with the DLL's
    // lifetime per the plugin-api contract; each copy is immediate.
    let (plugin_id, version, display_name, families_json, patterns_json) = unsafe {
        (
            copy_c_str(info.plugin_id),
            copy_c_str(info.plugin_version),
            copy_c_str(info.display_name),
            copy_c_str(info.families_json),
            copy_c_str(info.path_patterns_json),
        )
    };
    let plugin_id = plugin_id
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "has no plugin_id".to_string())?;
    let Some(version) = version else {
        return Err(format!("plugin '{plugin_id}' has no plugin_version"));
    };
    let evidence_platform = read_evidence_platform(info)?;
    let display_name = display_name.unwrap_or_else(|| plugin_id.clone());
    let families_json = families_json.unwrap_or_default();
    let families = match parse_families(&families_json) {
        Ok(families) if !families.is_empty() => families,
        _ => {
            return Err(format!(
                "plugin '{plugin_id}' declares no valid families_json"
            ));
        }
    };
    let patterns_json = patterns_json.unwrap_or_default();
    let patterns = parse_path_patterns(&patterns_json)
        .map_err(|error| format!("plugin '{plugin_id}' has invalid path_patterns_json: {error}"))?;
    Ok(PluginMeta {
        plugin_id,
        version,
        display_name,
        evidence_platform,
        families,
        patterns,
    })
}

/// Read the declared evidence platform without trusting enum layout: the
/// field is plugin-owned memory, so an out-of-range discriminant must be a
/// load error, never undefined behavior.
#[cfg(windows)]
fn read_evidence_platform(
    info: &plugin_api::MeowPluginInfo,
) -> Result<plugin_api::MeowEvidencePlatform, String> {
    // SAFETY: reinterpreting the repr(C) enum field as its u32 discriminant
    // is a plain read of four initialized bytes within `info`.
    let raw = unsafe { std::ptr::read((&raw const info.evidence_platform).cast::<u32>()) };
    match raw {
        0 => Ok(plugin_api::MeowEvidencePlatform::Windows),
        1 => Ok(plugin_api::MeowEvidencePlatform::Linux),
        other => Err(format!("unknown evidence platform discriminant {other}")),
    }
}

/// Copy a contract string field into an owned `String`.
///
/// # Safety
///
/// The caller guarantees `ptr` is either null or points to a NUL-terminated
/// UTF-8 buffer with the DLL's lifetime, per the plugin-api contract. The
/// pointer is never retained.
#[cfg(windows)]
unsafe fn copy_c_str(ptr: *const u8) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: upheld by the caller per the contract; we copy immediately.
    let bytes = unsafe { std::ffi::CStr::from_ptr(ptr.cast()) }.to_bytes();
    Some(String::from_utf8_lossy(bytes).into_owned())
}
