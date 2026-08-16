//! Export-side panic containment for plugins.
//!
//! Hard contract (ABI doc §8): on MSVC the host cannot catch a panic that
//! unwinds across the FFI boundary — the process aborts with a foreign
//! exception. Plugins statically link this crate, so `guarded_extract`'s
//! `catch_unwind` runs inside the DLL's own runtime where it does work.
//! Every `meow_plugin_extract` body must go through it.

use crate::types::{MeowExtractRequest, MeowExtractResponse, MeowStatus};
use std::ffi::CString;

/// Build an error response with a plugin-allocated, NUL-terminated message.
///
/// The host returns the message to `meow_plugin_free_buffer` with
/// `strlen + 1` as the length (ABI doc §5); `CString::into_raw` matches that
/// convention (the buffer's capacity equals its length with the NUL).
pub fn error_response(status: MeowStatus, message: &str) -> MeowExtractResponse {
    let message = CString::new(message).unwrap_or_else(|_| CString::new("error").unwrap());
    MeowExtractResponse {
        struct_size: size_of::<MeowExtractResponse>() as u32,
        status,
        payload: std::ptr::null_mut(),
        payload_len: 0,
        error_message: message.into_raw().cast(),
    }
}

/// Run `f` as the body of `meow_plugin_extract` with panic containment.
///
/// A panic inside `f` becomes an `InternalError` response; it never unwinds
/// past this function.
///
/// # Safety
///
/// `request` must be the pointer the host passed to `meow_plugin_extract`
/// (valid for the duration of the call).
pub unsafe fn guarded_extract(
    request: *const MeowExtractRequest,
    f: impl FnOnce(&MeowExtractRequest) -> MeowExtractResponse,
) -> MeowExtractResponse {
    if request.is_null() {
        return error_response(MeowStatus::InternalError, "null extract request");
    }
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: non-null checked above; validity for the call duration is
        // the caller's contract (the host's request pointer).
        let request = unsafe { &*request };
        f(request)
    }));
    outcome.unwrap_or_else(|_| {
        error_response(
            MeowStatus::InternalError,
            "plugin panicked during extraction",
        )
    })
}

/// Run `f` as the body of the optional `meow_plugin_action` export with
/// panic containment (same hard contract as [`guarded_extract`]).
///
/// The request is a length-delimited UTF-8 JSON document
/// (`{"action": "<id>", "params": {...}}`); `f` receives it as a byte slice
/// valid for the duration of the call. A panic inside `f` becomes an
/// `InternalError` response; it never unwinds past this function.
///
/// # Safety
///
/// `request`/`request_len` must be the pointer/length pair the host passed
/// to `meow_plugin_action` (valid for the duration of the call; may be null
/// only when `request_len` is 0).
pub unsafe fn guarded_action(
    request: *const u8,
    request_len: u64,
    f: impl FnOnce(&[u8]) -> MeowExtractResponse,
) -> MeowExtractResponse {
    if request.is_null() && request_len > 0 {
        return error_response(MeowStatus::InternalError, "null action request");
    }
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: null+0 yields an empty slice; otherwise the host contract
        // guarantees request_len readable bytes for the call duration.
        let bytes = if request.is_null() {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(request, request_len as usize) }
        };
        f(bytes)
    }));
    outcome.unwrap_or_else(|_| {
        error_response(MeowStatus::InternalError, "plugin panicked during action")
    })
}
