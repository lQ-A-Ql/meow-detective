//! Fixture plugin: exercises the optional action channel
//! (`meow_plugin_action`, ABI doc §3). Supports `describe`, an `echo`
//! action that returns its params, and a `panic` action proving the
//! plugin-side `guarded_action` self-catch maps panics to InternalError.

use plugin_api::{
    error_response, guarded_action, guarded_extract, MeowEvidencePlatform, MeowExtractRequest,
    MeowExtractResponse, MeowPluginInfo, MeowStatus, MEOW_PLUGIN_ABI_VERSION,
};

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_info() -> MeowPluginInfo {
    MeowPluginInfo {
        struct_size: std::mem::size_of::<MeowPluginInfo>() as u32,
        abi_version: MEOW_PLUGIN_ABI_VERSION,
        plugin_id: b"meow.fixture.action\0".as_ptr(),
        plugin_version: b"0.1.0\0".as_ptr(),
        display_name: b"Fixture Action\0".as_ptr(),
        evidence_platform: MeowEvidencePlatform::Windows,
        families_json: b"[\"FixtureAction\"]\0".as_ptr(),
        path_patterns_json: b"[\"*.afx\"]\0".as_ptr(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_extract(
    request: *const MeowExtractRequest,
) -> MeowExtractResponse {
    guarded_extract(request, |_request| {
        ok_response(br#"{"artifacts":[],"warnings":[]}"#)
    })
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_action(
    request: *const u8,
    request_len: u64,
) -> MeowExtractResponse {
    guarded_action(request, request_len, action_inner)
}

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_free_buffer(ptr: *mut u8, len: u64) {
    if !ptr.is_null() && len > 0 {
        // SAFETY: ptr/len originate from this DLL's own response allocation.
        drop(unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) });
    }
}

fn action_inner(request: &[u8]) -> MeowExtractResponse {
    let parsed: serde_json::Value = match serde_json::from_slice(request) {
        Ok(value) => value,
        Err(_) => return error_response(MeowStatus::ParseError, "request is not valid JSON"),
    };
    let action = parsed.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let params = parsed
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    match action {
        "describe" => ok_response(
            r#"{"actions":[{"id":"echo","label":"回显","description":"returns params","inputKind":"none"},{"id":"panic","label":"崩溃","description":"panics","inputKind":"none"}]}"#
                .as_bytes(),
        ),
        "echo" => ok_response(
            serde_json::json!({ "echo": params }).to_string().as_bytes(),
        ),
        "panic" => panic!("fixture action panic"),
        _ => error_response(MeowStatus::Unsupported, "unknown action"),
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
