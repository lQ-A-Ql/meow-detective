//! Optional plugin action channel (ABI doc §3 optional export).
//!
//! Actions:
//! - `describe`: self-description; declares the single action
//!   `recoverKeys` (`inputKind: "file"` — the host passes a memory-dump
//!   path).
//! - `recoverKeys`: stream-scans a memory dump for `x'<hex>'` SQLCipher
//!   key literals (`keyscan`) and validates every candidate against the
//!   host-supplied page 1 of each requested database
//!   (`dbPages: {"<dbName>": "<base64 page1>"}`), so only keys that
//!   actually decrypt a database are reported.
//!
//! Phase-one exception (author guide §2.1): the DLL performs the dump file
//! IO itself — the plugin is a local first-party tool and a multi-GB dump
//! cannot cross the bounded `extract` channel. The dump is only ever read.
//!
//! Keys are secrets: they are held in `Zeroizing` buffers, returned exactly
//! once in this action's response payload (the host persists them into the
//! ACL-protected case workspace), and never enter logs, warnings, or error
//! messages.

use plugin_api::{error_response, MeowExtractResponse, MeowStatus};
use serde_json::{Map, Value};
use std::path::Path;

use crate::keyscan;
use crate::sqlcipher4;

/// Action id: recover WeChat 4.x database keys from a memory dump.
pub const ACTION_RECOVER_KEYS: &str = "recoverKeys";

const DESCRIBE_PAYLOAD: &str = r#"{"actions":[{"id":"recoverKeys","label":"从内存镜像恢复数据库密钥","description":"扫描内存镜像中的 SQLCipher 密钥字面量，并用各加密数据库的第 1 页 HMAC 离线验证匹配（DLL 自行只读扫描 dump 文件，一期例外）","inputKind":"file"}]}"#;

/// Dispatch one action request body (`{"action": ..., "params": ...}`).
pub fn run(request: &[u8]) -> MeowExtractResponse {
    let parsed: Value = match serde_json::from_slice(request) {
        Ok(value) => value,
        Err(_) => {
            return error_response(MeowStatus::ParseError, "action request is not valid JSON")
        }
    };
    let action = parsed.get("action").and_then(Value::as_str).unwrap_or("");
    match action {
        "describe" => ok_response(DESCRIBE_PAYLOAD.as_bytes()),
        ACTION_RECOVER_KEYS => {
            let params = parsed.get("params").cloned().unwrap_or(Value::Null);
            match recover_keys(&params) {
                Ok(result) => ok_response(&result),
                Err(response) => response,
            }
        }
        _ => error_response(MeowStatus::Unsupported, "unknown action"),
    }
}

fn recover_keys(params: &Value) -> Result<Vec<u8>, MeowExtractResponse> {
    let dump_path = params.get("dumpPath").and_then(Value::as_str).unwrap_or("");
    if dump_path.is_empty() {
        return Err(error_response(
            MeowStatus::ParseError,
            "recoverKeys requires params.dumpPath",
        ));
    }
    let Some(db_pages) = params.get("dbPages").and_then(Value::as_object) else {
        return Err(error_response(
            MeowStatus::ParseError,
            "recoverKeys requires params.dbPages",
        ));
    };
    let media_sample = params.get("mediaSample").and_then(decode_media_sample);
    let scan = keyscan::scan_dump_for_keys_and_image(
        Path::new(dump_path),
        media_sample.as_ref().map(|sample| &sample.encrypted_block),
    )
    .map_err(|_| {
        error_response(
            MeowStatus::InternalError,
            "memory dump could not be read or scanned",
        )
    })?;
    let candidates = scan.candidates;
    let image_key = scan.image_key;
    let mut keys = Map::new();
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    for (db_name, page_b64) in db_pages {
        match decode_page1(page_b64) {
            Some(page1) => match candidates
                .iter()
                .find(|candidate| sqlcipher4::validate_page1(&candidate.key, &page1))
            {
                Some(candidate) => {
                    keys.insert(
                        db_name.clone(),
                        Value::String(keyscan::key_to_hex(&candidate.key)),
                    );
                    matched.push(Value::String(db_name.clone()));
                }
                None => unmatched.push(Value::String(db_name.clone())),
            },
            None => unmatched.push(Value::String(db_name.clone())),
        }
    }
    let mut result = Map::new();
    result.insert("keys".to_string(), Value::Object(keys));
    result.insert("matched".to_string(), Value::Array(matched));
    result.insert("unmatched".to_string(), Value::Array(unmatched));
    result.insert(
        "candidatesSeen".to_string(),
        Value::Number(candidates.len().into()),
    );
    if let Some(image_key) = image_key {
        result.insert(
            "imageKey".to_string(),
            Value::String(keyscan::image_key_to_hex(&image_key)),
        );
    }
    if let Some(sample) = media_sample {
        if let Some(xor_key) = sample.xor_key {
            result.insert(
                "imageXorKey".to_string(),
                Value::String(format!("{xor_key:02x}")),
            );
        }
    }
    Ok(Value::Object(result).to_string().into_bytes())
}

struct MediaSample {
    encrypted_block: [u8; 16],
    xor_key: Option<u8>,
}

fn decode_media_sample(value: &Value) -> Option<MediaSample> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.as_str()?)
        .ok()?;
    if bytes.len() < 31 || !bytes.starts_with(b"\x07\x08\x56\x32") {
        return None;
    }
    let mut encrypted_block = [0u8; 16];
    encrypted_block.copy_from_slice(&bytes[15..31]);
    let xor_key = (bytes.len() >= 2)
        .then(|| {
            let first = bytes[bytes.len() - 2] ^ 0xff;
            let second = bytes[bytes.len() - 1] ^ 0xd9;
            (first == second).then_some(first)
        })
        .flatten();
    Some(MediaSample {
        encrypted_block,
        xor_key,
    })
}

/// Base64-decode one database page-1 entry; only full pages can validate.
fn decode_page1(page_b64: &Value) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(page_b64.as_str()?)
        .ok()?;
    (bytes.len() >= sqlcipher4::PAGE_SZ).then_some(bytes)
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
