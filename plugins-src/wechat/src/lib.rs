//! WeChat (微信) 4.x parser plugin for Windows evidence — inventory plus
//! content deep-parse.
//!
//! Routes (see `route.rs`): the install `plugin_info.ini`, the roaming
//! `xwechat` side files (`cloud_account.txt`, `key_info.dat`,
//! `kvcomm/config.ini`), and the per-account `xwechat_files/<wxid>/
//! db_storage/<category>/*.db` databases. Databases encrypted with
//! WCDB/SQLCipher (all of them on WeChat 4.0.3.36+) are inventoried with an
//! explanatory warning unless a key is available: the key only lives in the
//! running process and is scrubbed, so recovery requires a memory dump
//! (`keyscan` implements the crash-dump scan + HMAC validation) and
//! injection through the `MEOW_WECHAT_KEYS` development channel
//! (`keyinject`). Plaintext databases — native (older builds) or decrypted
//! in memory — are deep-parsed via `sqlite3_deserialize` (read-only;
//! nothing touches the host disk) into contact/session/message/moment/
//! favorite content artifacts (see `content/`).
//!
//! Hard contract (design doc §8, plugins-src/README.md):
//! - every exported function self-catches panics; a panic escaping across
//!   the FFI boundary aborts the host process on MSVC;
//! - plugin-allocated buffers are reclaimed by `meow_plugin_free_buffer`
//!   (who allocates, frees); payloads free by explicit length, error
//!   messages are NUL-terminated and free at strlen+1;
//! - request pointers are valid only for the duration of the call.
//!
//! Redaction contract: `kTdiKeyCloudSession` values, `key_info.dat` bytes,
//! and any injected database keys are only tested for presence or consumed
//! in place — never emitted to payload, warnings, or logs.

/// Optional action channel (`recoverKeys` dump scan + page-1 validation).
pub mod action;
/// Database content parsers (contacts/sessions/messages/sns/favorites).
/// Public so the offline tooling binary can drive them directly.
pub mod content;
mod db;
pub use db::WeChatDb;
/// Development key-injection channel (`MEOW_WECHAT_KEYS`).
mod keyinject;
/// Crash-dump key recovery (`x'<hex>'` literal scan + HMAC validation).
/// Library-only within the extraction ABI (50MB cap); the host reaches it
/// through the `recoverKeys` action (`action`) and the offline tooling.
pub mod keyscan;
mod parse;
/// ABI payload builders; public for the offline tooling binary.
pub mod payload;
mod route;
/// SQLCipher 4 (WCDB) page decryption primitives for WeChat 4.x databases.
pub mod sqlcipher4;
/// WAL frame validation/decryption and merge into decrypted images.
pub mod walmerge;

use plugin_api::{
    error_response, guarded_action, guarded_extract, MeowCompanionFile, MeowEvidencePlatform,
    MeowExtractRequest, MeowExtractResponse, MeowPluginInfo, MeowStatus, MEOW_PLUGIN_ABI_VERSION,
};
use std::ffi::CStr;

use payload::Payload;
use route::Route;

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
        plugin_id: c"meow.plugin.wechat".as_ptr().cast(),
        plugin_version: c"0.4.2".as_ptr().cast(),
        display_name: c"微信".as_ptr().cast(),
        evidence_platform: MeowEvidencePlatform::Windows,
        families_json: c"[\"WeChatInstall\",\"WeChatAccount\",\"WeChatDatabase\",\"WeChatContact\",\"WeChatMessage\",\"WeChatSession\",\"WeChatMoment\",\"WeChatFavorite\",\"WeChatMedia\",\"WeChatSearchRecord\"]"
            .as_ptr()
            .cast(),
        path_patterns_json:
            c"[\"*.db\",\"plugin_info.ini\",\"cloud_account.txt\",\"key_info.dat\",\"config.ini\",\"/msg/attach/\",\"/sns/img/\"]"
                .as_ptr()
                .cast(),
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

/// ABI entry point (optional): self-describing JSON action channel.
///
/// # Safety
///
/// `request`/`request_len` must be null+0 or a valid pointer/length pair
/// referencing bytes that live for the call duration. The caller must return
/// the response buffers via `meow_plugin_free_buffer`.
#[no_mangle]
pub unsafe extern "C" fn meow_plugin_action(
    request: *const u8,
    request_len: u64,
) -> MeowExtractResponse {
    guarded_action(request, request_len, action::run)
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
    let file_id = match request_string(request.file_id, "file_id") {
        Ok(value) => value,
        Err(response) => return response,
    };
    // Imported paths may use either separator; normalize before routing.
    let normalized = file_path.replace('\\', "/");
    // Self-filter: non-WeChat paths (the host's `*.db` filter is wide) are
    // an empty-Ok, never an error.
    let route = match route::classify(&normalized) {
        Route::NotOurs => return ok_response(&Payload::empty().to_vec()),
        route => route,
    };
    if request.data.is_null() && request.data_len > 0 {
        return error_response(
            MeowStatus::InternalError,
            "null data pointer with nonzero length",
        );
    }
    // SAFETY: the host guarantees `data` points to `data_len` readable bytes
    // for the call duration; the parsers copy out and never retain the
    // pointer.
    let data = if request.data.is_null() {
        &[][..]
    } else {
        // SAFETY: pointer/length validity is guaranteed by the host for the call.
        unsafe { std::slice::from_raw_parts(request.data, request.data_len as usize) }
    };
    let mut payload = Payload::empty();
    match route {
        Route::InstallInfo => parse::install_info(&normalized, data, &mut payload),
        Route::CloudAccount => parse::cloud_account(data, &mut payload),
        Route::KeyInfo => parse::key_info(&normalized, request.data_len, &mut payload),
        Route::KvConfig => parse::kv_config(data, &mut payload),
        Route::LocalMedia => {
            if let Err(reason) = content::local_media::parse(&normalized, data, &mut payload) {
                return error_response(MeowStatus::ParseError, &reason);
            }
        }
        Route::Database => {
            let wal = match wal_companion(request, &normalized) {
                Ok(value) => value,
                Err(response) => return response,
            };
            if let Err(reason) = parse::database(&normalized, &file_id, data, wal, &mut payload) {
                return error_response(MeowStatus::ParseError, &reason);
            }
        }
        Route::NotOurs => unreachable!("NotOurs returns above"),
    }
    ok_response(&payload.to_vec())
}

fn wal_companion<'a>(
    request: &'a MeowExtractRequest,
    database_path: &str,
) -> Result<Option<&'a [u8]>, MeowExtractResponse> {
    if request.companion_count == 0 {
        return Ok(None);
    }
    if request.companions.is_null() || request.companion_count > 32 {
        return Err(error_response(
            MeowStatus::InternalError,
            "invalid companion file array",
        ));
    }
    let count = usize::try_from(request.companion_count)
        .map_err(|_| error_response(MeowStatus::InternalError, "companion count is too large"))?;
    // SAFETY: the host guarantees a contiguous array of `count` companion
    // records that remains valid for the extraction call.
    let companions = unsafe { std::slice::from_raw_parts(request.companions, count) };
    let wanted = format!("{database_path}-wal");
    for companion in companions {
        if companion.struct_size < std::mem::size_of::<MeowCompanionFile>() as u32 {
            continue;
        }
        let path = match request_string(companion.file_path, "companion.file_path") {
            Ok(value) => value.replace('\\', "/"),
            Err(response) => return Err(response),
        };
        if !path.eq_ignore_ascii_case(&wanted) {
            continue;
        }
        if companion.data.is_null() {
            if companion.data_len == 0 {
                return Ok(Some(&[]));
            }
            return Err(error_response(
                MeowStatus::InternalError,
                "null companion data pointer with nonzero length",
            ));
        }
        let len = usize::try_from(companion.data_len).map_err(|_| {
            error_response(MeowStatus::InternalError, "companion data is too large")
        })?;
        // SAFETY: the companion pointer/length is host-owned and valid for
        // the duration of this extraction call.
        return Ok(Some(unsafe {
            std::slice::from_raw_parts(companion.data, len)
        }));
    }
    Ok(None)
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
