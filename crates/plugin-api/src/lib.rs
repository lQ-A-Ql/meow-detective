//! Meow~Detective DLL parser plugin ABI contract.
//!
//! This crate is the single allowed dependency of parser plugins. It defines
//! the C ABI boundary: `#[repr(C)]` types (`types`), the ABI version and the
//! exported symbol names (`consts`). Bytes in, JSON records out; who
//! allocates, frees. See `docs/plugin-abi-contract-design.md` (local doc).

mod consts;
mod guard;
mod types;

pub use consts::{
    MEOW_PLUGIN_ABI_VERSION, MEOW_PLUGIN_ACTION_SYMBOL, MEOW_PLUGIN_EXTRACT_SYMBOL,
    MEOW_PLUGIN_FREE_BUFFER_SYMBOL, MEOW_PLUGIN_INFO_SYMBOL,
};
pub use guard::{error_response, guarded_action, guarded_extract};
pub use types::{
    MeowCompanionFile, MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse,
    MeowPluginInfo, MeowStatus,
};

/// Signature of the plugin's metadata entry point.
pub type MeowPluginInfoFn = unsafe extern "C" fn() -> MeowPluginInfo;
/// Signature of the plugin's extraction entry point.
pub type MeowPluginExtractFn =
    unsafe extern "C" fn(*const MeowExtractRequest) -> MeowExtractResponse;
/// Signature of the plugin's buffer release entry point.
pub type MeowPluginFreeBufferFn = unsafe extern "C" fn(*mut u8, u64);
/// Signature of the plugin's optional action entry point: a
/// length-delimited UTF-8 JSON request in, `MeowExtractResponse` out
/// (payload is the UTF-8 JSON action result, freed via
/// `meow_plugin_free_buffer`).
pub type MeowPluginActionFn = unsafe extern "C" fn(*const u8, u64) -> MeowExtractResponse;
