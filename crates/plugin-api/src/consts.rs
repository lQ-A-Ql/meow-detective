//! Meow~Detective DLL parser plugin ABI constants.
//!
//! See `docs/plugin-abi-contract-design.md` (local design doc) for the full
//! contract. Anything crossing the DLL boundary is a `#[repr(C)]` type from
//! `types`; this crate is the only Meow crate a plugin may depend on.

/// ABI major version. Bump on any breaking layout or semantic change.
pub const MEOW_PLUGIN_ABI_VERSION: u32 = 1;

/// Exported symbol: plugin metadata entry point.
pub const MEOW_PLUGIN_INFO_SYMBOL: &[u8] = b"meow_plugin_info\0";
/// Exported symbol: single-shot extraction entry point.
pub const MEOW_PLUGIN_EXTRACT_SYMBOL: &[u8] = b"meow_plugin_extract\0";
/// Exported symbol: free function for plugin-allocated buffers.
pub const MEOW_PLUGIN_FREE_BUFFER_SYMBOL: &[u8] = b"meow_plugin_free_buffer\0";
