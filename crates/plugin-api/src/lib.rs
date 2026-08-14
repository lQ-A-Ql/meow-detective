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
    MEOW_PLUGIN_ABI_VERSION, MEOW_PLUGIN_EXTRACT_SYMBOL, MEOW_PLUGIN_FREE_BUFFER_SYMBOL,
    MEOW_PLUGIN_INFO_SYMBOL,
};
pub use guard::{error_response, guarded_extract};
pub use types::{
    MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse, MeowPluginInfo, MeowStatus,
};

/// Signature of the plugin's metadata entry point.
pub type MeowPluginInfoFn = unsafe extern "C" fn() -> MeowPluginInfo;
/// Signature of the plugin's extraction entry point.
pub type MeowPluginExtractFn =
    unsafe extern "C" fn(*const MeowExtractRequest) -> MeowExtractResponse;
/// Signature of the plugin's buffer release entry point.
pub type MeowPluginFreeBufferFn = unsafe extern "C" fn(*mut u8, u64);
