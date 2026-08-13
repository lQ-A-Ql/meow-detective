//! Plugin DLL loading, symbol resolution and ABI handshake (design doc §5.2–5.4).

use super::extractor::PluginExtractor;
use std::path::PathBuf;

#[cfg(windows)]
use super::extractor::{parse_families, parse_path_patterns, PluginMeta};
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

/// Load every valid plugin DLL found under `dirs`. Invalid DLLs (load
/// failure, failed handshake, duplicate id, missing exports) are logged and
/// skipped; they never abort the remaining plugins.
#[cfg(windows)]
pub fn load_plugins_from_dirs(dirs: &[PathBuf]) -> Vec<PluginExtractor> {
    let mut seen_ids = HashSet::new();
    let mut loaded = Vec::new();
    for dir in dirs {
        for dll in enumerate_dlls(dir) {
            if let Some(extractor) = try_load_plugin(&dll, &mut seen_ids) {
                loaded.push(extractor);
            }
        }
    }
    loaded
}

/// Non-Windows hosts load no plugins; the desktop host is Windows-first and
/// this stub keeps the cross-platform target graph compiling.
#[cfg(not(windows))]
pub fn load_plugins_from_dirs(_dirs: &[PathBuf]) -> Vec<PluginExtractor> {
    Vec::new()
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
fn try_load_plugin(path: &Path, seen_ids: &mut HashSet<String>) -> Option<PluginExtractor> {
    let library = open_library(path)?;
    let info = read_plugin_info(path, &library)?;
    let meta = read_plugin_meta(path, &info)?;
    let extract = resolve_symbol::<plugin_api::MeowPluginExtractFn>(
        &library,
        plugin_api::MEOW_PLUGIN_EXTRACT_SYMBOL,
        path,
    )?;
    let free_buffer = resolve_symbol::<plugin_api::MeowPluginFreeBufferFn>(
        &library,
        plugin_api::MEOW_PLUGIN_FREE_BUFFER_SYMBOL,
        path,
    )?;
    if !seen_ids.insert(meta.plugin_id.clone()) {
        tracing::warn!(
            "duplicate plugin id '{}' ({}): refusing to load",
            meta.plugin_id,
            path.display()
        );
        return None;
    }
    tracing::info!(
        "loaded parser plugin '{}' v{} from {}",
        meta.plugin_id,
        meta.version,
        path.display()
    );
    Some(PluginExtractor::new(
        meta,
        PluginLibrary {
            _library: library,
            extract,
            free_buffer,
        },
    ))
}

#[cfg(windows)]
fn open_library(path: &Path) -> Option<libloading::os::windows::Library> {
    let absolute = match std::path::absolute(path) {
        Ok(absolute) => absolute,
        Err(error) => {
            tracing::warn!("plugin path {} not absolutized: {}", path.display(), error);
            return None;
        }
    };
    // SAFETY: the path is absolute and the flags confine dependent-DLL
    // resolution to System32 and the plugin's own directory, eliminating the
    // CWD/PATH search-order hijack surface. The handle is owned by the
    // returned Library and unloaded on drop.
    match unsafe {
        libloading::os::windows::Library::load_with_flags(
            &absolute,
            libloading::os::windows::LOAD_LIBRARY_SEARCH_SYSTEM32
                | libloading::os::windows::LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
        )
    } {
        Ok(library) => Some(library),
        Err(error) => {
            tracing::warn!("plugin {} failed to load: {}", path.display(), error);
            None
        }
    }
}

#[cfg(windows)]
fn resolve_symbol<T: Copy>(
    library: &libloading::os::windows::Library,
    symbol: &[u8],
    path: &Path,
) -> Option<T> {
    // SAFETY: the symbol names and their fn pointer types are the plugin-api
    // contract constants; the pointer is copied out while the library mapping
    // stays alive inside PluginLibrary.
    match unsafe { library.get::<T>(symbol) } {
        Ok(symbol) => Some(*symbol),
        Err(error) => {
            let name = String::from_utf8_lossy(symbol);
            tracing::warn!(
                "plugin {} misses required export '{}': {}",
                path.display(),
                name.trim_end_matches('\0'),
                error
            );
            None
        }
    }
}

#[cfg(windows)]
fn read_plugin_info(
    path: &Path,
    library: &libloading::os::windows::Library,
) -> Option<plugin_api::MeowPluginInfo> {
    let info_fn = resolve_symbol::<plugin_api::MeowPluginInfoFn>(
        library,
        plugin_api::MEOW_PLUGIN_INFO_SYMBOL,
        path,
    )?;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        // SAFETY: the contract guarantees meow_plugin_info is callable right
        // after load and returns plain data whose pointers live with the DLL.
        || unsafe { info_fn() },
    ));
    let info = match outcome {
        Ok(info) => info,
        Err(_) => {
            tracing::warn!("plugin {} panicked in meow_plugin_info", path.display());
            return None;
        }
    };
    let expected = std::mem::size_of::<plugin_api::MeowPluginInfo>() as u32;
    if info.struct_size < expected {
        tracing::warn!(
            "plugin {} reports struct_size {} < expected {}; refusing to load",
            path.display(),
            info.struct_size,
            expected
        );
        return None;
    }
    if info.abi_version != plugin_api::MEOW_PLUGIN_ABI_VERSION {
        tracing::warn!(
            "plugin {} ABI version {} != host {}; refusing to load",
            path.display(),
            info.abi_version,
            plugin_api::MEOW_PLUGIN_ABI_VERSION
        );
        return None;
    }
    Some(info)
}

#[cfg(windows)]
fn read_plugin_meta(path: &Path, info: &plugin_api::MeowPluginInfo) -> Option<PluginMeta> {
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
    let plugin_id = plugin_id.filter(|id| !id.trim().is_empty());
    let Some(plugin_id) = plugin_id else {
        tracing::warn!(
            "plugin {} has no plugin_id; refusing to load",
            path.display()
        );
        return None;
    };
    let Some(version) = version else {
        tracing::warn!(
            "plugin '{}' ({}) has no plugin_version; refusing to load",
            plugin_id,
            path.display()
        );
        return None;
    };
    let display_name = display_name.unwrap_or_else(|| plugin_id.clone());
    let families_json = families_json.unwrap_or_default();
    let families = match parse_families(&families_json) {
        Ok(families) if !families.is_empty() => families,
        _ => {
            tracing::warn!(
                "plugin '{}' ({}) declares no valid families_json; refusing to load",
                plugin_id,
                path.display()
            );
            return None;
        }
    };
    let patterns_json = patterns_json.unwrap_or_default();
    let patterns = match parse_path_patterns(&patterns_json) {
        Ok(patterns) => patterns,
        Err(error) => {
            tracing::warn!(
                "plugin '{}' ({}) has invalid path_patterns_json: {}; refusing to load",
                plugin_id,
                path.display(),
                error
            );
            return None;
        }
    };
    Some(PluginMeta {
        plugin_id,
        version,
        display_name,
        families,
        patterns,
    })
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
