//! Fixture plugin: violates contract §3 by letting a panic unwind across the
//! DLL boundary (`extern "C-unwind"`). Used by the subprocess test that
//! documents the MSVC cross-runtime limitation: the host's `catch_unwind`
//! classifies a panic from the DLL's own Rust runtime as a foreign exception
//! and aborts the process. Plugins MUST catch their own panics.

use plugin_api::{
    MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse, MeowPluginInfo,
    MEOW_PLUGIN_ABI_VERSION,
};

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_info() -> MeowPluginInfo {
    MeowPluginInfo {
        struct_size: std::mem::size_of::<MeowPluginInfo>() as u32,
        abi_version: MEOW_PLUGIN_ABI_VERSION,
        plugin_id: b"meow.fixture.panic-unwind\0".as_ptr(),
        plugin_version: b"0.1.0\0".as_ptr(),
        display_name: b"Fixture Panic Unwind\0".as_ptr(),
        evidence_platform: MeowEvidencePlatform::Windows,
        families_json: b"[\"Fixture\"]\0".as_ptr(),
        path_patterns_json: b"[\"*.pnu\"]\0".as_ptr(),
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn meow_plugin_extract(
    _request: *const MeowExtractRequest,
) -> MeowExtractResponse {
    panic!("fixture cross-boundary panic");
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_free_buffer(ptr: *mut u8, len: u64) {
    if !ptr.is_null() && len > 0 {
        // SAFETY: ptr/len originate from this DLL's own response allocation.
        drop(unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) });
    }
}
