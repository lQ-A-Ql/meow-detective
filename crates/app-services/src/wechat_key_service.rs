//! WeChat (微信) database-key recovery service.
//!
//! Drives the WeChat plugin's `recoverKeys` action (ABI doc §3 optional
//! export): the service collects the page-1 (4096 bytes, bounded) of every
//! `xwechat_files/.../db_storage/*.db` entry in the data source through the
//! evidence reader, hands them plus the investigator-selected memory dump
//! path to the plugin (the DLL scans the dump itself — phase-one exception
//! for a local first-party tool), and persists the recovered keys to
//! `<case_root>/derived/wechat-keys/keys.json` (temp file + atomic rename,
//! then the caller-provided ACL restriction).
//!
//! Key discipline: keys cross the plugin boundary once, are written to the
//! ACL-protected case workspace, and are returned only in the recovery DTO so
//! the local investigator can see them in the plugin title. They never enter
//! logs, audit details, artifacts, plugin metadata, or reports.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::file_service::SourceReadContext;
use crate::plugin_action_service::{call_plugin_action, PluginActionError};
use crate::source_db;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use serde_json::{Map, Value};
use transport::dto::{WeChatKeyRecoveryResultDto, WeChatRecoveredKeyDto};

mod keys_file;

pub(super) use keys_file::write_keys_file;

/// Plugin id of the WeChat parser plugin.
pub const WECHAT_PLUGIN_ID: &str = "meow.plugin.wechat";
/// Action id on the WeChat plugin's action channel.
const RECOVER_KEYS_ACTION: &str = "recoverKeys";
/// Mirrors the plugin's `keyinject::KEYS_ENV` contract (the plugin crate is
/// not linkable from the host; the literal is the contract).
const WECHAT_KEYS_ENV: &str = "MEOW_WECHAT_KEYS";
const IMAGE_KEY_ENTRY: &str = "__wechat_image_key_v2";
const IMAGE_XOR_KEY_ENTRY: &str = "__wechat_image_xor_key_v2";
/// file_entries path fragments selecting WeChat 4.x database candidates.
const DB_PATH_FRAGMENTS: [&str; 3] = ["xwechat_files", "db_storage", ".db"];
const PAGE1_BYTES: usize = 4096;
const MEDIA_SAMPLE_MAX_BYTES: usize = 1024 * 1024;
/// Memory-dump extensions accepted for recovery (loose but never a dir).
const DUMP_EXTENSIONS: [&str; 2] = ["dmp", "raw"];

/// Recovered-keys file: `<case_root>/derived/wechat-keys/keys.json`, in the
/// keyinject format `{"<fileEntryId>": "<hex>"}` the plugin's injection
/// channel reads during analysis extraction. File ids avoid collisions
/// between accounts and data sources; the plugin retains legacy fallbacks.
pub fn keys_file_path(case_root: &Path) -> PathBuf {
    case_root
        .join("derived")
        .join("wechat-keys")
        .join("keys.json")
}

/// Recover WeChat database keys for one data source from a memory dump.
///
/// `restrict_acl` tightens the written keys file to the current user (the
/// Tauri shell passes `platform_security::restrict_file_to_current_user`;
/// the service layer must not depend on the shell).
pub fn recover_wechat_keys(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    dump_path: &Path,
    restrict_acl: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<WeChatKeyRecoveryResultDto, PluginActionError> {
    validate_dump_path(dump_path)?;
    let source = source_db::open_ready_source_by_id(case_conn, case_root, case_id, data_source_id)?;
    let db_pages = collect_db_pages(
        &source.connection,
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )?;
    let media_sample = collect_media_sample(
        &source.connection,
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )?;
    if db_pages.pages.is_empty() {
        return Ok(WeChatKeyRecoveryResultDto {
            candidates_seen: 0,
            recovered_count: 0,
            matched_db_names: Vec::new(),
            unmatched_db_names: Vec::new(),
            recovered_keys: Vec::new(),
        });
    }
    let mut params = Map::new();
    params.insert(
        "dumpPath".to_string(),
        Value::String(dump_path.to_string_lossy().into_owned()),
    );
    params.insert("dbPages".to_string(), Value::Object(db_pages.pages));
    if let Some(sample) = media_sample {
        params.insert("mediaSample".to_string(), sample);
    }
    let response = call_plugin_action(
        WECHAT_PLUGIN_ID,
        RECOVER_KEYS_ACTION,
        &Value::Object(params),
    )?;
    let mut outcome = RecoveryOutcome::parse(response)?;
    outcome.retain_valid_keys(&db_pages.display_names);
    outcome.apply_display_names(&db_pages.display_names);
    if outcome.recovered_count() > 0 {
        let path = keys_file_path(case_root);
        write_keys_file(&path, &outcome.keys)?;
        restrict_acl(&path)?;
    }
    audit_recovery(case_conn, case_id, data_source_id, &outcome);
    Ok(outcome.into_dto())
}

fn collect_media_sample(
    source_conn: &Connection,
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<Option<Value>, PluginActionError> {
    use base64::Engine as _;

    let entries = FileRepo::new(source_conn)
        .find_by_path_fragments(data_source_id, &["xwechat_files", "msg/attach", "_t.dat"])?;
    let mut reader =
        SourceReadContext::new(source_conn, case_conn, case_root, case_id, data_source_id);
    for entry in entries {
        let size = entry
            .size
            .and_then(|size| usize::try_from(size).ok())
            .unwrap_or(MEDIA_SAMPLE_MAX_BYTES)
            .min(MEDIA_SAMPLE_MAX_BYTES);
        let Ok(sample) = reader.read_file_header_by_id(&entry.id, size) else {
            continue;
        };
        if sample.len() >= 31 && sample.starts_with(b"\x07\x08\x56\x32") {
            return Ok(Some(Value::String(
                base64::engine::general_purpose::STANDARD.encode(sample),
            )));
        }
    }
    Ok(None)
}

/// Dump path admission: must exist, be a regular file, and carry a
/// `.dmp`/`.raw` extension. The dump is only ever read (by the plugin).
fn validate_dump_path(dump_path: &Path) -> Result<(), PluginActionError> {
    let metadata = std::fs::metadata(dump_path)
        .map_err(|_| PluginActionError::InvalidInput("dump path does not exist".to_string()))?;
    if !metadata.is_file() {
        return Err(PluginActionError::InvalidInput(
            "dump path must be a regular file".to_string(),
        ));
    }
    let extension_ok = dump_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            DUMP_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        });
    if !extension_ok {
        return Err(PluginActionError::InvalidInput(
            "dump path must have a .dmp or .raw extension".to_string(),
        ));
    }
    Ok(())
}

/// Read page 1 of every WeChat database candidate through the
/// source-bound evidence reader. Unreadable/short entries are skipped with
/// a warning (they simply stay unrecovered).
struct DbPageCollection {
    pages: Map<String, Value>,
    display_names: BTreeMap<String, String>,
}

impl DbPageCollection {
    fn empty() -> Self {
        Self {
            pages: Map::new(),
            display_names: BTreeMap::new(),
        }
    }
}

fn collect_db_pages(
    source_conn: &Connection,
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<DbPageCollection, PluginActionError> {
    use base64::Engine as _;
    let entries =
        FileRepo::new(source_conn).find_by_path_fragments(data_source_id, &DB_PATH_FRAGMENTS)?;
    let mut db_pages = DbPageCollection::empty();
    if entries.is_empty() {
        return Ok(db_pages);
    }
    let mut reader =
        SourceReadContext::new(source_conn, case_conn, case_root, case_id, data_source_id);
    for entry in &entries {
        match reader.read_file_header_by_id(&entry.id, PAGE1_BYTES) {
            Ok(page) if page.len() >= PAGE1_BYTES => {
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(&page[..PAGE1_BYTES]);
                let key = entry.id.0.clone();
                db_pages.pages.insert(key.clone(), Value::String(encoded));
                db_pages
                    .display_names
                    .insert(key, entry.path.replace('\\', "/"));
            }
            Ok(_) => {
                tracing::warn!(path = %entry.path, "WeChat database page 1 short read; skipped");
            }
            Err(error) => {
                tracing::warn!(path = %entry.path, "WeChat database page 1 unreadable: {error}");
            }
        }
    }
    Ok(db_pages)
}

/// Parsed plugin response. `keys` is consumed by `write_keys_file`; the
/// verified subset is also copied into the explicit local recovery DTO.
struct RecoveryOutcome {
    keys: Map<String, Value>,
    candidates_seen: u64,
    matched_db_names: Vec<String>,
    unmatched_db_names: Vec<String>,
    recovered_keys: Vec<WeChatRecoveredKeyDto>,
}

impl RecoveryOutcome {
    fn parse(response: Value) -> Result<Self, PluginActionError> {
        let mut keys = response
            .get("keys")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(image_key) = response.get("imageKey").and_then(Value::as_str) {
            keys.insert(
                IMAGE_KEY_ENTRY.to_string(),
                Value::String(image_key.to_string()),
            );
        }
        if let Some(xor_key) = response.get("imageXorKey").and_then(Value::as_str) {
            keys.insert(
                IMAGE_XOR_KEY_ENTRY.to_string(),
                Value::String(xor_key.to_string()),
            );
        }
        let candidates_seen = response
            .get("candidatesSeen")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let matched_db_names = string_list(response.get("matched"));
        let unmatched_db_names = string_list(response.get("unmatched"));
        Ok(Self {
            keys,
            candidates_seen,
            matched_db_names,
            unmatched_db_names,
            recovered_keys: Vec::new(),
        })
    }

    fn recovered_count(&self) -> u64 {
        u64::try_from(
            self.keys
                .keys()
                .filter(|key| key.as_str() != IMAGE_XOR_KEY_ENTRY)
                .count(),
        )
        .unwrap_or(u64::MAX)
    }

    fn retain_valid_keys(&mut self, requested: &BTreeMap<String, String>) {
        self.keys.retain(|file_id, value| {
            let Some(value) = value.as_str() else {
                return false;
            };
            match file_id.as_str() {
                IMAGE_KEY_ENTRY => is_valid_hex(value, 32),
                IMAGE_XOR_KEY_ENTRY => is_valid_hex(value, 2),
                _ => requested.contains_key(file_id) && is_valid_hex(value, 64),
            }
        });
        self.matched_db_names
            .retain(|file_id| self.keys.contains_key(file_id));
        self.unmatched_db_names
            .retain(|file_id| requested.contains_key(file_id));
    }

    fn apply_display_names(&mut self, display_names: &BTreeMap<String, String>) {
        self.recovered_keys = self
            .keys
            .iter()
            .filter(|(file_id, _)| {
                file_id.as_str() != IMAGE_KEY_ENTRY && file_id.as_str() != IMAGE_XOR_KEY_ENTRY
            })
            .filter_map(|(file_id, value)| {
                Some(WeChatRecoveredKeyDto {
                    database_name: display_names.get(file_id)?.clone(),
                    key_hex: value.as_str()?.to_string(),
                })
            })
            .collect();
        if let Some(key_hex) = self.keys.get(IMAGE_KEY_ENTRY).and_then(Value::as_str) {
            self.recovered_keys.push(WeChatRecoveredKeyDto {
                database_name: "微信图片密钥".to_string(),
                key_hex: key_hex.to_string(),
            });
        }
        for name in &mut self.matched_db_names {
            if let Some(display) = display_names.get(name) {
                *name = display.clone();
            }
        }
        for name in &mut self.unmatched_db_names {
            if let Some(display) = display_names.get(name) {
                *name = display.clone();
            }
        }
    }

    fn into_dto(self) -> WeChatKeyRecoveryResultDto {
        WeChatKeyRecoveryResultDto {
            candidates_seen: self.candidates_seen,
            recovered_count: self.recovered_count(),
            matched_db_names: self.matched_db_names,
            unmatched_db_names: self.unmatched_db_names,
            recovered_keys: self.recovered_keys,
        }
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn is_valid_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Audit the recovery run with counts only — never key material or dump
/// paths. Audit failures are non-fatal, matching the plugin audit trail.
fn audit_recovery(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    outcome: &RecoveryOutcome,
) {
    let details = serde_json::json!({
        "dataSourceId": data_source_id.0,
        "candidatesSeen": outcome.candidates_seen,
        "recoveredCount": outcome.recovered_count(),
        "unmatchedCount": outcome.unmatched_db_names.len(),
    })
    .to_string();
    let result = AuditRepo::new(case_conn).log(
        Some(&case_id.0),
        "system",
        &AuditAction::PluginKeyRecovery,
        Some(WECHAT_PLUGIN_ID),
        &details,
    );
    if let Err(error) = result {
        tracing::warn!("wechat key recovery audit event could not be recorded: {error}");
    }
}

/// RAII guard pointing the WeChat plugin's key-injection channel
/// (`MEOW_WECHAT_KEYS`) at the recovered keys file for the duration of one
/// analysis extraction run; without a keys file nothing changes.
///
/// Process-level env caveat: this is a single-user desktop application and
/// analysis extraction is admitted through a bounded scheduler (a single
/// extraction slot, see `extraction/scheduler.rs`), so the set/restore
/// window cannot race a concurrent extraction.
pub(crate) struct WeChatKeysEnvGuard {
    previous: Option<std::ffi::OsString>,
}

impl WeChatKeysEnvGuard {
    pub(crate) fn activate(case_root: &Path) -> Option<Self> {
        let path = keys_file_path(case_root);
        if !path.is_file() {
            return None;
        }
        let previous = std::env::var_os(WECHAT_KEYS_ENV);
        std::env::set_var(WECHAT_KEYS_ENV, &path);
        Some(Self { previous })
    }
}

impl Drop for WeChatKeysEnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(WECHAT_KEYS_ENV, value),
            None => std::env::remove_var(WECHAT_KEYS_ENV),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/wechat_key_service.rs"]
mod tests;
