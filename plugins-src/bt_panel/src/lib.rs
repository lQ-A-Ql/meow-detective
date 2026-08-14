//! BT Panel (宝塔面板) parser plugin — phase-2 first field plugin.
//!
//! Parses the panel's SQLite databases under `/www/server/panel/data/`
//! (`default.db` plus the per-domain `db/*.db` split) into ABI payload JSON
//! (design doc §4). Evidence bytes are deserialized into an in-memory,
//! read-only SQLite database; nothing is written to the host disk.
//!
//! Hard contract (design doc §8, plugins-src/README.md):
//! - every exported function self-catches panics; a panic escaping across
//!   the FFI boundary aborts the host process on MSVC;
//! - plugin-allocated buffers are reclaimed by `meow_plugin_free_buffer`
//!   (who allocates, frees); payloads free by explicit length, error
//!   messages are NUL-terminated and free at strlen+1;
//! - request pointers are valid only for the duration of the call.
//!
//! Redaction contract: panel password hashes / salts / FTP and database
//! passwords are never emitted — only a `hasPasswordHash`/`hasPassword`
//! boolean and, for panel accounts, the recognized algorithm shape.

mod db;
mod parse;
mod payload;
mod time;

use plugin_api::{
    error_response, guarded_extract, MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse,
    MeowPluginInfo, MeowStatus, MEOW_PLUGIN_ABI_VERSION,
};
use std::ffi::CStr;

use payload::Payload;

/// Panel data directory marker. Imported paths may carry a `[P{n}]`
/// partition prefix and may drop the leading slash (root folding), so the
/// self-filter matches the bare marker anywhere in the logical path.
const PANEL_DATA_DIR: &str = "www/server/panel/data/";

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
        plugin_id: c"meow.plugin.bt_panel".as_ptr().cast(),
        plugin_version: c"0.1.0".as_ptr().cast(),
        display_name: c"宝塔面板".as_ptr().cast(),
        evidence_platform: MeowEvidencePlatform::Linux,
        families_json: c"[\"BtPanelAccount\",\"BtPanelSite\",\"BtPanelDatabase\",\"BtPanelFtp\",\"BtPanelFirewall\",\"BtPanelTask\",\"BtPanelLog\"]".as_ptr().cast(),
        path_patterns_json: c"[\"*.db\"]".as_ptr().cast(),
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
    let file_path = match request_string(request.file_path, "file_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    // Self-filter: only panel data databases are ours; every other path
    // (including unknown DB names inside the panel directory) is an
    // empty-Ok, never an error.
    if !file_path.contains(PANEL_DATA_DIR) {
        return ok_response(&Payload::empty().to_vec());
    }
    let basename = file_path.rsplit('/').next().unwrap_or(&file_path);
    let routes: &[&str] = match basename.to_ascii_lowercase().as_str() {
        "default.db" => &[
            parse::ACCOUNTS,
            parse::SITES,
            parse::DATABASES,
            parse::FTPS,
            parse::FIREWALL,
            parse::CRONTAB,
            parse::LOGS,
        ],
        // Modern panels (9.x) moved the login accounts into db/panel.db;
        // default.db then only holds the factory template row.
        "panel.db" => &[parse::ACCOUNTS],
        "site.db" => &[parse::SITES],
        "database.db" => &[parse::DATABASES],
        "ftp.db" => &[parse::FTPS],
        "firewall.db" => &[parse::FIREWALL],
        "crontab.db" => &[parse::CRONTAB],
        "log.db" => &[parse::LOGS],
        _ => return ok_response(&Payload::empty().to_vec()),
    };
    if request.data.is_null() && request.data_len > 0 {
        return error_response(
            MeowStatus::InternalError,
            "null data pointer with nonzero length",
        );
    }
    // SAFETY: the host guarantees `data` points to `data_len` readable bytes
    // for the call duration; `PanelDb::from_bytes` copies out and never
    // retains the pointer.
    let data = unsafe { std::slice::from_raw_parts(request.data, request.data_len as usize) };
    let panel_db = match db::PanelDb::from_bytes(data) {
        Ok(panel_db) => panel_db,
        Err(reason) => {
            return error_response(MeowStatus::ParseError, &reason);
        }
    };
    let mut payload = Payload::empty();
    for route in routes {
        if let Err(reason) = parse::run(route, &panel_db, &mut payload) {
            return error_response(MeowStatus::ParseError, &reason);
        }
    }
    ok_response(&payload.to_vec())
}

fn request_string(pointer: *const u8, field: &str) -> Result<String, MeowExtractResponse> {
    if pointer.is_null() {
        return Err(error_response(
            MeowStatus::InternalError,
            "null request string pointer",
        ));
    }
    if field.is_empty() {
        return Err(error_response(
            MeowStatus::InternalError,
            "empty request field name",
        ));
    }
    // SAFETY: contract guarantees a NUL-terminated UTF-8 string valid for
    // the call duration; it is copied into an owned String.
    Ok(unsafe { CStr::from_ptr(pointer.cast()) }
        .to_string_lossy()
        .into_owned())
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
