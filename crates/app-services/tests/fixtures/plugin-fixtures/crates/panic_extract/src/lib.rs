//! Fixture plugin: `meow_plugin_extract` panics internally, catches it with
//! its own runtime (contract §3: panics must not cross the boundary) and
//! reports `InternalError`. The host maps this to a warning-level error and
//! keeps the batch alive.

use plugin_api::{
    MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse, MeowPluginInfo, MeowStatus,
    MEOW_PLUGIN_ABI_VERSION,
};

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_info() -> MeowPluginInfo {
    MeowPluginInfo {
        struct_size: std::mem::size_of::<MeowPluginInfo>() as u32,
        abi_version: MEOW_PLUGIN_ABI_VERSION,
        plugin_id: b"meow.fixture.panic\0".as_ptr(),
        plugin_version: b"0.1.0\0".as_ptr(),
        display_name: b"Fixture Panic\0".as_ptr(),
        evidence_platform: MeowEvidencePlatform::Windows,
        families_json: b"[\"Fixture\"]\0".as_ptr(),
        path_patterns_json: b"[\"*.pnc\"]\0".as_ptr(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_extract(
    _request: *const MeowExtractRequest,
) -> MeowExtractResponse {
    let outcome = std::panic::catch_unwind(|| panic!("fixture panic"));
    if outcome.is_err() {
        return error_response("fixture panic");
    }
    error_response("unreachable")
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_free_buffer(ptr: *mut u8, len: u64) {
    if !ptr.is_null() && len > 0 {
        // SAFETY: ptr/len originate from this DLL's own response allocation.
        drop(unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) });
    }
}

fn error_response(message: &str) -> MeowExtractResponse {
    let mut buffer = message.as_bytes().to_vec();
    buffer.push(0);
    buffer.shrink_to_fit();
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    MeowExtractResponse {
        struct_size: std::mem::size_of::<MeowExtractResponse>() as u32,
        status: MeowStatus::InternalError,
        payload: std::ptr::null_mut(),
        payload_len: 0,
        error_message: ptr,
    }
}
