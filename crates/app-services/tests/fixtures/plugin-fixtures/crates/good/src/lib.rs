//! Fixture plugin: the happy path. Returns a valid payload with one artifact,
//! one timeline event and one warning (plugin system M2 tests).

use plugin_api::{
    MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse, MeowPluginInfo, MeowStatus,
    MEOW_PLUGIN_ABI_VERSION,
};

const PAYLOAD: &[u8] = br#"{
  "artifacts": [
    {
      "family": "Fixture",
      "title": "fixture artifact",
      "summary": "ok",
      "confidence": 0.9,
      "attrs": { "origin": "plugin" }
    }
  ],
  "timelineEvents": [
    {
      "timestampUtc": "2026-08-01T12:00:00Z",
      "eventType": "Execution",
      "description": "fixture event",
      "attrs": {}
    }
  ],
  "warnings": ["fixture warning"]
}"#;

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_info() -> MeowPluginInfo {
    MeowPluginInfo {
        struct_size: std::mem::size_of::<MeowPluginInfo>() as u32,
        abi_version: MEOW_PLUGIN_ABI_VERSION,
        plugin_id: b"meow.fixture.good\0".as_ptr(),
        plugin_version: b"0.1.0\0".as_ptr(),
        display_name: b"Fixture Good\0".as_ptr(),
        evidence_platform: MeowEvidencePlatform::Windows,
        families_json: b"[\"Fixture\"]\0".as_ptr(),
        path_patterns_json: b"[\"*.mfx\", \"fixture-exact.bin\"]\0".as_ptr(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_extract(
    request: *const MeowExtractRequest,
) -> MeowExtractResponse {
    if request.is_null() {
        return error_response("null request");
    }
    // SAFETY: the host guarantees the request is valid for the call duration.
    let request = unsafe { &*request };
    let _ = (request.file_path, request.file_id, request.data_len);
    ok_response(PAYLOAD)
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_free_buffer(ptr: *mut u8, len: u64) {
    if !ptr.is_null() && len > 0 {
        // SAFETY: ptr/len originate from this DLL's own response allocation.
        drop(unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) });
    }
}

fn ok_response(payload: &[u8]) -> MeowExtractResponse {
    let mut buffer = payload.to_vec();
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

fn error_response(message: &str) -> MeowExtractResponse {
    let mut buffer = message.as_bytes().to_vec();
    buffer.push(0);
    buffer.shrink_to_fit();
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    MeowExtractResponse {
        struct_size: std::mem::size_of::<MeowExtractResponse>() as u32,
        status: MeowStatus::ParseError,
        payload: std::ptr::null_mut(),
        payload_len: 0,
        error_message: ptr,
    }
}
