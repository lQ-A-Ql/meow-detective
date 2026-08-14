//! Tests for the export-side panic containment helper.

use plugin_api::{
    error_response, guarded_extract, MeowExtractRequest, MeowExtractResponse, MeowStatus,
};
use std::ffi::CStr;

fn ok_response() -> MeowExtractResponse {
    MeowExtractResponse {
        struct_size: size_of::<MeowExtractResponse>() as u32,
        status: MeowStatus::Ok,
        payload: std::ptr::null_mut(),
        payload_len: 0,
        error_message: std::ptr::null_mut(),
    }
}

fn request() -> MeowExtractRequest {
    MeowExtractRequest {
        struct_size: size_of::<MeowExtractRequest>() as u32,
        file_path: b"x.pf\0".as_ptr(),
        file_id: b"ds:1:1\0".as_ptr(),
        data: b"data".as_ptr(),
        data_len: 4,
    }
}

#[test]
fn guarded_extract_passes_through_ok() {
    let req = request();
    let response = unsafe { guarded_extract(&req, |_| ok_response()) };
    assert_eq!(response.status, MeowStatus::Ok);
}

#[test]
fn guarded_extract_turns_panics_into_internal_error() {
    let req = request();
    let response = unsafe { guarded_extract(&req, |_| panic!("boom")) };
    assert_eq!(response.status, MeowStatus::InternalError);
    assert!(!response.error_message.is_null());
    let message = unsafe { CStr::from_ptr(response.error_message.cast()) };
    assert!(message.to_string_lossy().contains("panicked"));
    // Return the message to the plugin-side convention for freeing.
    let len = message.to_bytes_with_nul().len();
    unsafe { drop(Vec::from_raw_parts(response.error_message, len, len)) };
}

#[test]
fn guarded_extract_rejects_null_requests() {
    let response = unsafe { guarded_extract(std::ptr::null(), |_| ok_response()) };
    assert_eq!(response.status, MeowStatus::InternalError);
    let len = unsafe { CStr::from_ptr(response.error_message.cast()) }
        .to_bytes_with_nul()
        .len();
    unsafe { drop(Vec::from_raw_parts(response.error_message, len, len)) };
}

#[test]
fn error_response_carries_the_message() {
    let response = error_response(MeowStatus::ParseError, "bad magic");
    assert_eq!(response.status, MeowStatus::ParseError);
    let message = unsafe { CStr::from_ptr(response.error_message.cast()) };
    assert_eq!(message.to_string_lossy(), "bad magic");
    let len = message.to_bytes_with_nul().len();
    unsafe { drop(Vec::from_raw_parts(response.error_message, len, len)) };
}
