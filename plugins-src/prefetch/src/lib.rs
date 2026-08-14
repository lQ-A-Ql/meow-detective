//! Prefetch parser plugin — plugin system M3 pilot.
//!
//! Runs the built-in track-A `artifacts_windows::PrefetchExtractor` inside
//! this DLL and serializes the captured sink into the ABI payload JSON
//! (design doc §4), so the built-in and plugin channels produce identical
//! artifacts by construction.
//!
//! Hard contract (design doc §8, plugins-src/README.md):
//! - every exported function self-catches panics; a panic escaping across
//!   the FFI boundary aborts the host process on MSVC;
//! - plugin-allocated buffers are reclaimed by `meow_plugin_free_buffer`
//!   (who allocates, frees); payloads free by explicit length, error
//!   messages are NUL-terminated and free at strlen+1;
//! - request pointers are valid only for the duration of the call.

use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::{Artifact, FileEntryId, TimelineEvent};
use plugin_api::{
    error_response, guarded_extract, MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse,
    MeowPluginInfo, MeowStatus, MEOW_PLUGIN_ABI_VERSION,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::ffi::CStr;

/// ABI entry point: plugin metadata handshake.
///
/// # Safety
///
/// Purely returns pointers to `'static` data inside this DLL; safe to call
/// at any time after load.
#[no_mangle]
pub unsafe extern "C" fn meow_plugin_info() -> MeowPluginInfo {
    MeowPluginInfo {
        struct_size: std::mem::size_of::<MeowPluginInfo>() as u32,
        abi_version: MEOW_PLUGIN_ABI_VERSION,
        plugin_id: c"meow.plugin.prefetch".as_ptr().cast(),
        plugin_version: c"0.1.0".as_ptr().cast(),
        display_name: c"Prefetch".as_ptr().cast(),
        evidence_platform: MeowEvidencePlatform::Windows,
        families_json: c"[\"Prefetch\"]".as_ptr().cast(),
        path_patterns_json: c"[\"*.pf\"]".as_ptr().cast(),
    }
}

/// ABI entry point: single-shot extraction.
///
/// # Safety
///
/// `request` must be null or point to a valid `MeowExtractRequest` whose
/// pointer fields reference buffers that live for the call duration. The
/// caller must return the response buffers via `meow_plugin_free_buffer`.
#[no_mangle]
pub unsafe extern "C" fn meow_plugin_extract(
    request: *const MeowExtractRequest,
) -> MeowExtractResponse {
    guarded_extract(request, extract_inner)
}

/// ABI entry point: reclaim a plugin-allocated response buffer.
///
/// # Safety
///
/// `ptr`/`len` must be an exact pointer/length pair previously handed out
/// by `meow_plugin_extract` (payload by explicit length, error message at
/// strlen+1), and each pair may be freed at most once.
#[no_mangle]
pub unsafe extern "C" fn meow_plugin_free_buffer(ptr: *mut u8, len: u64) {
    if !ptr.is_null() && len > 0 {
        // SAFETY: ptr/len originate from this DLL's own response allocation
        // (payload Vec or error-message CString, both length == capacity).
        drop(unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) });
    }
}

/// Panic containment lives in `plugin_api::guarded_extract` (the plugin
/// statically links it, so the catch happens in this DLL's own runtime).
fn extract_inner(request: &MeowExtractRequest) -> MeowExtractResponse {
    let data = match request_data(request) {
        Ok(data) => data,
        Err(response) => return response,
    };
    let (file_path, file_id) = match request_strings(request) {
        Ok(strings) => strings,
        Err(response) => return response,
    };
    let ctx = ArtifactContext {
        file_id: FileEntryId(file_id),
        file_path,
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = match artifacts_windows::PrefetchExtractor.run(ctx, &mut sink) {
        Ok(report) => report,
        Err(error) => return error_response(MeowStatus::InternalError, &error),
    };
    if report.artifacts_found == 0 && !report.errors.is_empty() {
        // The parser failed on the whole input (corrupt/truncated/unknown
        // variant): surface it as a typed ParseError rather than an empty Ok.
        return error_response(MeowStatus::ParseError, &report.errors.join("; "));
    }
    ok_response(&build_payload(&sink, report.errors))
}

fn request_data(request: &MeowExtractRequest) -> Result<Vec<u8>, MeowExtractResponse> {
    if request.data.is_null() && request.data_len > 0 {
        return Err(error_response(
            MeowStatus::InternalError,
            "null data pointer with nonzero length",
        ));
    }
    if request.data.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: the host guarantees `data` points to `data_len` readable bytes
    // for the call duration; we copy out and never retain the pointer.
    Ok(unsafe { std::slice::from_raw_parts(request.data, request.data_len as usize) }.to_vec())
}

fn request_strings(request: &MeowExtractRequest) -> Result<(String, String), MeowExtractResponse> {
    if request.file_path.is_null() || request.file_id.is_null() {
        return Err(error_response(
            MeowStatus::InternalError,
            "null file_path or file_id pointer",
        ));
    }
    // SAFETY: contract guarantees NUL-terminated UTF-8 strings valid for the
    // call duration; both are copied into owned Strings.
    let (path, id) = unsafe {
        (
            CStr::from_ptr(request.file_path.cast())
                .to_string_lossy()
                .into_owned(),
            CStr::from_ptr(request.file_id.cast())
                .to_string_lossy()
                .into_owned(),
        )
    };
    Ok((path, id))
}

/// Serialize the captured sink into the ABI payload JSON (design doc §4).
/// Provenance fields are deliberately absent: the host overwrites them.
fn build_payload(sink: &VecSink, warnings: Vec<String>) -> Vec<u8> {
    let payload = Payload {
        artifacts: sink.artifacts.iter().map(payload_artifact).collect(),
        timeline_events: sink
            .timeline_events
            .iter()
            .map(payload_timeline_event)
            .collect(),
        warnings,
    };
    serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec())
}

fn payload_artifact(artifact: &Artifact) -> PayloadArtifact {
    PayloadArtifact {
        family: artifact.family.clone(),
        title: artifact.title.clone(),
        summary: artifact.summary.clone(),
        confidence: artifact.confidence,
        attrs: artifact.attrs.clone().into_iter().collect(),
    }
}

fn payload_timeline_event(event: &TimelineEvent) -> PayloadTimelineEvent {
    PayloadTimelineEvent {
        timestamp_utc: event.timestamp.to_rfc3339(),
        event_type: event.event_type.clone(),
        description: event.description.clone(),
        attrs: event.attrs.clone().into_iter().collect(),
    }
}

#[derive(Serialize)]
struct Payload {
    artifacts: Vec<PayloadArtifact>,
    #[serde(rename = "timelineEvents")]
    timeline_events: Vec<PayloadTimelineEvent>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct PayloadArtifact {
    family: String,
    title: String,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence: Option<f32>,
    attrs: Map<String, Value>,
}

#[derive(Serialize)]
struct PayloadTimelineEvent {
    #[serde(rename = "timestampUtc")]
    timestamp_utc: String,
    #[serde(rename = "eventType")]
    event_type: String,
    description: String,
    attrs: Map<String, Value>,
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

#[cfg(test)]
mod tests;
