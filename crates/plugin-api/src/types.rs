//! `#[repr(C)]` types crossing the plugin DLL boundary.
//!
//! Rules (see `docs/plugin-abi-contract-design.md`):
//! - only `repr(C)` structs, raw pointers and lengths cross the boundary;
//!   no Rust heap type (`String`/`Vec`/`Box`/trait object) may appear here;
//! - who allocates, frees: plugin-allocated buffers are returned to the
//!   plugin via `meow_plugin_free_buffer`; host-allocated input stays owned
//!   by the host;
//! - every struct carries `struct_size` for forward-compatible evolution.

/// Evidence platform of the analyzed data source (NOT the runtime platform;
/// the host always runs on Windows).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeowEvidencePlatform {
    Windows = 0,
    Linux = 1,
}

/// Plugin metadata returned by `meow_plugin_info`.
///
/// All string fields are NUL-terminated UTF-8 pointers with the DLL's
/// lifetime ('static inside the plugin); the host copies before use.
#[repr(C)]
pub struct MeowPluginInfo {
    /// `size_of::<MeowPluginInfo>()` as built into the plugin.
    pub struct_size: u32,
    /// Must equal `MEOW_PLUGIN_ABI_VERSION` or the host refuses to load.
    pub abi_version: u32,
    /// e.g. "meow.plugin.prefetch" (dotted/kebab).
    pub plugin_id: *const u8,
    /// SemVer string; recorded as `extractor_version` in provenance.
    pub plugin_version: *const u8,
    /// Human-facing module name, e.g. "微信".
    pub display_name: *const u8,
    pub evidence_platform: MeowEvidencePlatform,
    /// JSON array of declared artifact families, e.g. `["Prefetch"]`.
    pub families_json: *const u8,
    /// JSON array of path match patterns (suffix or exact name), e.g. `["*.pf"]`.
    pub path_patterns_json: *const u8,
}

/// Extraction request. `data` points into a host-owned buffer valid only
/// for the duration of the call; the plugin must not retain it.
#[repr(C)]
pub struct MeowExtractRequest {
    pub struct_size: u32,
    /// Logical evidence path (with `[P{n}]` partition prefix), NUL-terminated.
    pub file_path: *const u8,
    /// FileEntryId ("ds:<dataSourceId>:<localId>"), NUL-terminated.
    pub file_id: *const u8,
    pub data: *const u8,
    pub data_len: u64,
}

/// Extraction status, mapped by the host onto `ErrorCategory`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeowStatus {
    Ok = 0,
    /// Input cannot be parsed reliably (corrupt/truncated/unknown variant).
    ParseError = 1,
    /// Recognized as the plugin's target but the content type is unsupported.
    Unsupported = 2,
    /// Plugin-internal failure.
    InternalError = 3,
}

/// Extraction response. `payload`/`error_message` are plugin-allocated and
/// must be returned with `meow_plugin_free_buffer` after the host reads them.
#[repr(C)]
pub struct MeowExtractResponse {
    pub struct_size: u32,
    pub status: MeowStatus,
    /// UTF-8 JSON payload (schema in the design doc §4); may be null on error.
    pub payload: *mut u8,
    pub payload_len: u64,
    /// Optional, must not contain sensitive host paths; may be null.
    pub error_message: *mut u8,
}
