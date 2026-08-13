//! Fixture plugin: valid payload whose artifact family is not in the declared
//! `families_json` set. The host must drop the artifact with a warning.

use plugin_api::{
    MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse, MeowPluginInfo, MeowStatus,
    MEOW_PLUGIN_ABI_VERSION,
};

const PAYLOAD: &[u8] = br#"{
  "artifacts": [
    { "family": "Undeclared", "title": "sneaky", "summary": "bad", "attrs": {} }
  ]
}"#;

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_info() -> MeowPluginInfo {
    MeowPluginInfo {
        struct_size: std::mem::size_of::<MeowPluginInfo>() as u32,
        abi_version: MEOW_PLUGIN_ABI_VERSION,
        plugin_id: b"meow.fixture.undeclared-family\0".as_ptr(),
        plugin_version: b"0.1.0\0".as_ptr(),
        display_name: b"Fixture Undeclared Family\0".as_ptr(),
        evidence_platform: MeowEvidencePlatform::Windows,
        families_json: b"[\"Fixture\"]\0".as_ptr(),
        path_patterns_json: b"[\"*.udf\"]\0".as_ptr(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_extract(
    _request: *const MeowExtractRequest,
) -> MeowExtractResponse {
    let mut buffer = PAYLOAD.to_vec();
    buffer.shrink_to_fit();
    let len = buffer.len() as u64;
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    MeowExtractResponse {
        struct_size: std::mem::size_of::<MeowExtractResponse>() as u32,
        status: MeowStatus::Ok,
        payload: ptr,
        payload_len: len,
        error_message: std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_free_buffer(ptr: *mut u8, len: u64) {
    if !ptr.is_null() && len > 0 {
        // SAFETY: ptr/len originate from this DLL's own response allocation.
        drop(unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) });
    }
}
