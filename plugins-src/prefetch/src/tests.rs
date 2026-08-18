//! Plugin-internal unit tests (M3): panic self-capture, buffer free
//! round-trips, and a synthetic v30 fixture through the exported entry point.

use super::*;
use std::ffi::CString;

/// Minimal valid uncompressed Prefetch v30 sample (84-byte header + 220-byte
/// file-info section). The equivalent builder lives in
/// `crates/app-services/tests/prefetch_plugin_regression.rs` for the
/// dual-channel test; keep both in sync.
pub(crate) fn sample_prefetch_v30() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&30u32.to_le_bytes()); // format version
    data.extend_from_slice(b"SCCA"); // uncompressed signature
    data.extend_from_slice(&0u32.to_le_bytes()); // unknown
    data.extend_from_slice(&12345u32.to_le_bytes()); // executable file size
    let mut name = [0u8; 60];
    for (index, unit) in "CMD.EXE".encode_utf16().enumerate() {
        name[index * 2..index * 2 + 2].copy_from_slice(&unit.to_le_bytes());
    }
    data.extend_from_slice(&name);
    data.extend_from_slice(&0x0A1B2C3Du32.to_le_bytes()); // hash
    data.extend_from_slice(&0u32.to_le_bytes()); // flags
    debug_assert_eq!(data.len(), 84);
    let mut info = vec![0u8; 220];
    // 8 FILETIME run-time slots at offset 44; slot 0 = 2026-01-02T03:04:05Z.
    let unix = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
        .expect("fixture timestamp")
        .timestamp();
    let filetime = ((unix + 11_644_473_600) as u64) * 10_000_000;
    info[44..52].copy_from_slice(&filetime.to_le_bytes());
    // run_count at offset 124 (doubles as the standard-section count probe).
    info[124..128].copy_from_slice(&3u32.to_le_bytes());
    // standard-section hash probe at offset 136 must be <= total length so
    // the v30 layout resolver picks the 220-byte info section.
    info[136..140].copy_from_slice(&100u32.to_le_bytes());
    data.extend_from_slice(&info);
    data
}

fn request_for(data: &[u8]) -> (CString, CString, MeowExtractRequest) {
    let path = CString::new("[P0]/Windows/Prefetch/CMD.EXE-0A1B2C3D.pf").expect("path");
    let id = CString::new("ds:1:pf-1").expect("id");
    let request = MeowExtractRequest {
        struct_size: std::mem::size_of::<MeowExtractRequest>() as u32,
        file_path: path.as_ptr().cast(),
        file_id: id.as_ptr().cast(),
        data: data.as_ptr(),
        data_len: data.len() as u64,
        companions: std::ptr::null(),
        companion_count: 0,
    };
    (path, id, request)
}

/// Read and free the response buffers exactly the way the host does.
unsafe fn drain_response(response: &MeowExtractResponse) -> (Option<Vec<u8>>, Option<String>) {
    let payload = if response.payload.is_null() {
        None
    } else {
        // SAFETY: payload/payload_len were allocated by this plugin.
        let bytes =
            unsafe { std::slice::from_raw_parts(response.payload, response.payload_len as usize) }
                .to_vec();
        unsafe { meow_plugin_free_buffer(response.payload, response.payload_len) };
        Some(bytes)
    };
    let error = if response.error_message.is_null() {
        None
    } else {
        // SAFETY: error_message is a NUL-terminated plugin allocation.
        let (text, len) = unsafe {
            let message = CStr::from_ptr(response.error_message.cast());
            (
                message.to_string_lossy().into_owned(),
                message.to_bytes_with_nul().len() as u64,
            )
        };
        unsafe { meow_plugin_free_buffer(response.error_message, len) };
        Some(text)
    };
    (payload, error)
}

#[test]
fn info_reports_expected_metadata() {
    let info = unsafe { meow_plugin_info() };
    assert_eq!(info.abi_version, MEOW_PLUGIN_ABI_VERSION);
    assert_eq!(
        info.struct_size,
        std::mem::size_of::<MeowPluginInfo>() as u32
    );
    assert_eq!(info.evidence_platform, MeowEvidencePlatform::Windows);
    unsafe {
        assert_eq!(
            CStr::from_ptr(info.plugin_id.cast()),
            c"meow.plugin.prefetch"
        );
        assert_eq!(CStr::from_ptr(info.families_json.cast()), c"[\"Prefetch\"]");
        assert_eq!(
            CStr::from_ptr(info.path_patterns_json.cast()),
            c"[\"*.pf\"]"
        );
    }
}

#[test]
fn panic_inside_extract_is_self_caught() {
    let request = MeowExtractRequest {
        struct_size: std::mem::size_of::<MeowExtractRequest>() as u32,
        file_path: std::ptr::null(),
        file_id: std::ptr::null(),
        data: std::ptr::null(),
        data_len: 0,
        companions: std::ptr::null(),
        companion_count: 0,
    };
    let response = unsafe { guarded_extract(&request, |_| panic!("boom")) };
    assert_eq!(response.status, MeowStatus::InternalError);
    assert!(response.payload.is_null());
    let (_, error) = unsafe { drain_response(&response) };
    assert!(error.expect("error message").contains("panicked"));
}

#[test]
fn null_request_and_null_data_fail_closed() {
    let response = unsafe { meow_plugin_extract(std::ptr::null()) };
    assert_eq!(response.status, MeowStatus::InternalError);
    let (_, error) = unsafe { drain_response(&response) };
    assert!(error.is_some());

    let path = CString::new("x.pf").expect("path");
    let id = CString::new("ds:1:1").expect("id");
    let request = MeowExtractRequest {
        struct_size: std::mem::size_of::<MeowExtractRequest>() as u32,
        file_path: path.as_ptr().cast(),
        file_id: id.as_ptr().cast(),
        data: std::ptr::null(),
        data_len: 16,
        companions: std::ptr::null(),
        companion_count: 0,
    };
    let response = unsafe { meow_plugin_extract(&request) };
    assert_eq!(response.status, MeowStatus::InternalError);
    let (_, error) = unsafe { drain_response(&response) };
    assert!(error.expect("error message").contains("null data pointer"));
}

#[test]
fn valid_fixture_produces_prefetch_payload() {
    let data = sample_prefetch_v30();
    let (_path, _id, request) = request_for(&data);
    let response = unsafe { meow_plugin_extract(&request) };
    assert_eq!(response.status, MeowStatus::Ok);
    let (payload, error) = unsafe { drain_response(&response) };
    assert!(error.is_none());
    let payload: Value = serde_json::from_slice(&payload.expect("payload")).expect("valid JSON");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "Prefetch");
    assert_eq!(artifact["attrs"]["executable"], "CMD.EXE");
    assert_eq!(artifact["attrs"]["run_count"], 3);
    assert_eq!(artifact["attrs"]["format_version"], 30);
    assert_eq!(artifact["attrs"]["hash"], "0A1B2C3D");
    assert_eq!(artifact["attrs"]["file_size"], 12345);
    assert_eq!(
        artifact["attrs"]["last_run_times"]
            .as_array()
            .expect("run times")
            .len(),
        1
    );
    assert_eq!(
        payload["timelineEvents"].as_array().expect("events").len(),
        1
    );
    assert_eq!(
        payload["timelineEvents"][0]["eventType"],
        "PROGRAM_EXECUTION"
    );
}

#[test]
fn truncated_input_is_parse_error_not_abort() {
    let mut data = 30u32.to_le_bytes().to_vec();
    data.extend_from_slice(b"SCCA"); // signature present, header truncated
    let (_path, _id, request) = request_for(&data);
    let response = unsafe { meow_plugin_extract(&request) };
    assert_eq!(response.status, MeowStatus::ParseError);
    let (_, error) = unsafe { drain_response(&response) };
    assert!(error.expect("error message").contains("truncated"));
}

#[test]
fn non_prefetch_input_is_silent_ok() {
    let (_path, _id, request) = request_for(b"definitely not a prefetch file");
    let response = unsafe { meow_plugin_extract(&request) };
    assert_eq!(response.status, MeowStatus::Ok);
    let (payload, _) = unsafe { drain_response(&response) };
    let payload: Value = serde_json::from_slice(&payload.expect("payload")).expect("valid JSON");
    assert_eq!(payload["artifacts"].as_array().expect("artifacts").len(), 0);
}
