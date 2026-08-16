//! Per-route parsers for WeChat (微信) 4.x on Windows.
//!
//! Coverage model (see docs/parser-support-matrix.md):
//! - plaintext side files (`plugin_info.ini`, `cloud_account.txt`,
//!   `key_info.dat`, `kvcomm/config.ini`) are fully parsed into
//!   WeChatInstall / WeChatAccount inventory artifacts;
//! - `db_storage/*.db` files are inventoried as WeChatDatabase with the
//!   wxid/category/name/size and an `encrypted` flag;
//! - encrypted (WCDB/SQLCipher) databases stop at inventory plus an
//!   explanatory warning — WeChat 4.0.3.36+ keeps the key only in the
//!   running process and scrubs it, so a pure disk image cannot recover it
//!   offline;
//! - a plaintext database (older builds, other images) is deep-parsed:
//!   table list plus per-table row counts (a missing/unreadable table is a
//!   ParseError, absent tables simply do not appear).
//!
//! Redaction: `kTdiKeyCloudSession` and `key_info.dat` contents are only
//! tested for presence; their bytes never enter the payload.

use serde_json::{Map, Value};

use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};
use crate::route;

/// Encryption note attached to every encrypted WeChatDatabase artifact.
const ENCRYPTION_WARNING: &str = "WCDB/SQLCipher 加密；微信 4.0.3.36+ 密钥仅存在于运行进程内存且会主动清除，纯磁盘镜像无法离线恢复，需要外部密钥材料方可内容提取";

/// Bound on the emitted table list so a pathological database cannot blow
/// up the payload; the host reads the whole payload into memory.
const MAX_TABLES: usize = 100;

/// `plugin_info.ini` → WeChatInstall: install version/path from the
/// logical path plus the INI plugin-version table.
pub fn install_info(path: &str, data: &[u8], payload: &mut Payload) {
    let ini = parse_ini(&String::from_utf8_lossy(data));
    let mut attrs = new_attrs();
    if let Some(version) = route::install_version(path) {
        attrs.insert(
            "installVersion".to_string(),
            Value::String(version.to_string()),
        );
    }
    if let Some(dir) = route::install_path(path) {
        attrs.insert("installPath".to_string(), Value::String(dir));
    }
    attrs.insert("pluginVersions".to_string(), Value::Object(ini));
    let version = route::install_version(path).unwrap_or("<unknown>");
    payload.artifact(
        "WeChatInstall",
        format!("微信 {version}"),
        format!(
            "安装目录插件清单（{} 项）",
            attrs_len(&attrs, "pluginVersions")
        ),
        attrs,
    );
}

fn attrs_len(attrs: &Map<String, Value>, key: &str) -> usize {
    match attrs.get(key) {
        Some(Value::Object(map)) => map.len(),
        _ => 0,
    }
}

/// `cloud_account.txt` → WeChatAccount: only the presence of the cloud
/// session key is reported; the value itself is redacted.
pub fn cloud_account(data: &[u8], payload: &mut Payload) {
    let kv = parse_key_values(&String::from_utf8_lossy(data));
    let session = kv
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("kTdiKeyCloudSession"))
        .map(|(_, value)| value.as_str());
    let has_session = session.is_some_and(|value| !value.is_empty());
    let mut attrs = new_attrs();
    attrs.insert("hasCloudSession".to_string(), Value::Bool(has_session));
    payload.artifact(
        "WeChatAccount",
        "微信云会话",
        if has_session {
            "kTdiKeyCloudSession 存在（值已脱敏）"
        } else {
            "kTdiKeyCloudSession 为空或缺失"
        },
        attrs,
    );
}

/// `login/<wxid>/key_info.dat` → WeChatAccount: login key-material
/// inventory; the blob is encrypted and never copied out.
pub fn key_info(path: &str, data_len: u64, payload: &mut Payload) {
    let wxid = route::parent_segment(path).unwrap_or("<unknown>");
    let mut attrs = new_attrs();
    attrs.insert("wxid".to_string(), Value::String(wxid.to_string()));
    attrs.insert("keyInfoPresent".to_string(), Value::Bool(true));
    attrs.insert("sizeBytes".to_string(), Value::from(data_len));
    payload.artifact(
        "WeChatAccount",
        format!("登录密钥材料（{wxid}）"),
        format!("登录密钥材料存在但已加密（{data_len} 字节）"),
        attrs,
    );
}

/// `ilink/kvcomm/config.ini` → WeChatInstall: kvcomm client settings.
pub fn kv_config(data: &[u8], payload: &mut Payload) {
    let settings = parse_ini(&String::from_utf8_lossy(data));
    let count = settings.len();
    let mut attrs = new_attrs();
    attrs.insert("settings".to_string(), Value::Object(settings));
    payload.artifact(
        "WeChatInstall",
        "微信 ilink kvcomm 配置",
        format!("kvcomm 配置项（{count} 项）"),
        attrs,
    );
}

/// `xwechat_files/<wxid>/db_storage/<category>/*.db` → WeChatDatabase.
/// Plaintext databases are deep-parsed; encrypted ones are inventoried
/// with the encryption warning. Corrupt plaintext input is a ParseError.
pub fn database(path: &str, data: &[u8], payload: &mut Payload) -> Result<(), String> {
    let wxid = route::segment_after(path, "xwechat_files").unwrap_or("<unknown>");
    let category = route::segment_after(path, "db_storage").unwrap_or("<unknown>");
    let db_name = route::basename(path);
    let encrypted = data.len() < 16 || &data[..16] != b"SQLite format 3\0";

    let mut attrs = new_attrs();
    attrs.insert("wxid".to_string(), Value::String(wxid.to_string()));
    attrs.insert("category".to_string(), Value::String(category.to_string()));
    attrs.insert("dbName".to_string(), Value::String(db_name.to_string()));
    attrs.insert("sizeBytes".to_string(), Value::from(data.len() as u64));
    attrs.insert("encrypted".to_string(), Value::Bool(encrypted));

    if encrypted {
        payload.warn(ENCRYPTION_WARNING);
        payload.artifact(
            "WeChatDatabase",
            format!("{category}/{db_name}"),
            format!("WCDB/SQLCipher 加密数据库（{} 字节），仅盘点", data.len()),
            attrs,
        );
        return Ok(());
    }

    // Plaintext path: inventory the schema and count rows per table. The
    // row-count pass doubles as the defensive message/contact table probe —
    // whatever tables exist (message/session/contact/sns variants) are
    // counted, and absent ones simply do not appear.
    let db = WeChatDb::from_bytes(data)?;
    let tables = db.table_list()?;
    if tables.len() > MAX_TABLES {
        payload.warn(format!(
            "table list truncated: {} tables, emitting first {MAX_TABLES}",
            tables.len()
        ));
    }
    let tables: Vec<String> = tables.into_iter().take(MAX_TABLES).collect();
    let mut row_counts = Map::new();
    for table in &tables {
        row_counts.insert(table.clone(), Value::from(db.row_count(table)?));
    }
    let table_count = tables.len();
    attrs.insert(
        "tableList".to_string(),
        Value::Array(tables.into_iter().map(Value::String).collect()),
    );
    attrs.insert("rowCounts".to_string(), Value::Object(row_counts));
    attrs.insert("tableCount".to_string(), Value::from(table_count as u64));
    payload.artifact(
        "WeChatDatabase",
        format!("{category}/{db_name}"),
        format!("明文 SQLite 数据库，{table_count} 张表"),
        attrs,
    );
    Ok(())
}

/// Loose INI parser: `[section]` prefixes keys as `section.key`; `;`/`#`
/// lines and blank lines are skipped; last value wins on duplicates.
fn parse_ini(text: &str) -> Map<String, Value> {
    let mut out = Map::new();
    let mut section = String::new();
    for (key, value) in parse_key_values(text) {
        if key == "<<" {
            // Section marker emitted by parse_key_values for `[name]` lines.
            section = value;
            continue;
        }
        let full_key = if section.is_empty() {
            key
        } else {
            format!("{section}.{key}")
        };
        out.insert(full_key, Value::String(value));
    }
    out
}

/// Line-based `key=value` pairs; `[section]` lines are yielded as the
/// sentinel key `<<`. Handles BOM, CRLF, and surrounding whitespace.
fn parse_key_values(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push(("<<".to_string(), name.trim().to_string()));
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            out.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    out
}
