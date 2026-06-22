use super::reader::RegistryHiveReader;
use super::txlog_util::{
    find_best_txlog_match, find_best_txlog_match_user_assist, parse_user_assist_binary,
    txlog_data_to_string,
};
use super::*;
use crate::registry::RegistryError;

// ── NTUSER.DAT field extraction ──────────────────────────────────────────────

pub fn extract_ntuser_fields(bytes: &[u8], hive_path: &str) -> Result<NtuserInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = NtuserInfo::default();
    let parser = "registry.ntuser";

    info.run_keys = extract_run_keys(&hive, hive_path, parser, &mut info.warnings);
    info.recent_docs = extract_recent_docs(&hive, hive_path, parser, &mut info.warnings);
    info.ua_entries = extract_user_assist(&hive, hive_path, parser, &mut info.warnings);
    info.typed_urls = extract_typed_urls(&hive, hive_path, parser, &mut info.warnings);
    info.word_wheel_query = extract_word_wheel_query(&hive, hive_path, parser, &mut info.warnings);
    info.mount_points = extract_mount_points(&hive, hive_path, parser, &mut info.warnings);
    info.open_save_mru = extract_open_save_mru(&hive, hive_path, parser, &mut info.warnings);
    info.last_visited_mru = extract_last_visited_mru(&hive, hive_path, parser, &mut info.warnings);
    info.run_mru = extract_run_mru(&hive, hive_path, parser, &mut info.warnings);
    info.default_browser = extract_default_browser(&hive);

    Ok(info)
}

/// Like [`extract_ntuser_fields`], but after standard extraction checks a
/// transaction log for more recent writes to Run / RunOnce keys and TypedURLs.
pub fn extract_ntuser_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<NtuserInfo, RegistryError> {
    let mut info = extract_ntuser_fields(bytes, hive_path)?;
    let txlog = parse_transaction_log(txlog_data)?;
    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    // Override Run / RunOnce commands.
    for run_key in &mut info.run_keys {
        let best =
            find_best_txlog_match(&txlog.transactions, &run_key.key_path, &run_key.value_name);
        if let Some(txn) = best {
            if let Some(new_cmd) = txn.data_after.as_deref().and_then(txlog_data_to_string) {
                run_key.command = new_cmd;
                run_key.timestamp = txn.timestamp.map(|dt| dt.to_rfc3339());
                ts_infos.push(TxlogTimestampInfo {
                    field_name: format!("RunKey[{}]", run_key.value_name),
                    hive_timestamp: None,
                    txlog_timestamp: txn.timestamp,
                    txlog_used: true,
                });
                txlog_applied = true;
            }
        }
    }

    // Apply txlog overrides to UserAssist entries.
    for ua_entry in &mut info.ua_entries {
        // ROT13 is its own inverse: the value name stored in the registry is
        // the ROT13-encoded version of executable_path.
        let encoded_name = rot13_decode(&ua_entry.executable_path);
        let best = find_best_txlog_match_user_assist(&txlog.transactions, &encoded_name);
        if let Some(txn) = best {
            if let Some(data) = &txn.data_after {
                if let Some((run_count, session_id, focus_time, filetime)) =
                    parse_user_assist_binary(data)
                {
                    ua_entry.run_count = run_count;
                    ua_entry.session_id = session_id;
                    ua_entry.focus_time_ms = focus_time as u64;
                    ua_entry.last_run = windows_filetime_to_rfc3339(filetime);
                    ts_infos.push(TxlogTimestampInfo {
                        field_name: format!("UserAssist[{}]", ua_entry.executable_path),
                        hive_timestamp: None,
                        txlog_timestamp: txn.timestamp,
                        txlog_used: true,
                    });
                    txlog_applied = true;
                }
            }
        }
    }

    info.txlog_applied = txlog_applied;
    info.txlog_timestamps = ts_infos;
    Ok(info)
}

// ── Run / RunOnce ────────────────────────────────────────────────────────────

fn extract_run_keys(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<RegistryRunKey> {
    let mut keys = Vec::new();
    let base = &["Software", "Microsoft", "Windows", "CurrentVersion"];
    for suffix in &["Run", "RunOnce"] {
        let mut full: Vec<&str> = base.to_vec();
        full.push(suffix);
        keys.extend(extract_run_keys_at(
            hive, hive_path, parser, &full, warnings,
        ));
    }
    keys
}

fn extract_run_keys_at(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    key_path: &[&str],
    warnings: &mut Vec<String>,
) -> Vec<RegistryRunKey> {
    let key_path_str = key_path.join("\\");
    let nk = match hive.navigate_to(key_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("{key_path_str} parse error: {err}"));
            return Vec::new();
        }
    };
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            warnings.push(format!("{key_path_str} values parse error: {err}"));
            return Vec::new();
        }
    };
    values
        .into_iter()
        .filter_map(|(name, value)| match value {
            RegistryValue::String(command) if !command.trim().is_empty() => Some(RegistryRunKey {
                key_path: key_path_str.clone(),
                value_name: name,
                command,
                timestamp: None,
                scope: "user".to_string(),
            }),
            _ => None,
        })
        .collect()
}

// ── RecentDocs MRU ───────────────────────────────────────────────────────────

fn extract_recent_docs(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<RecentDoc> {
    let recent_docs_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "RecentDocs",
    ];
    let nk = match hive.navigate_to(recent_docs_path) {
        Ok(Some(nk)) => nk,
        // A missing RecentDocs key is normal for the Default profile or
        // freshly-created user accounts; do not surface it as a warning.
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("RecentDocs parse error: {err}"));
            return Vec::new();
        }
    };
    let subkey_names = match hive.read_subkey_names_from_nk(&nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("RecentDocs subkeys error: {err}"));
            return Vec::new();
        }
    };
    let mut docs = Vec::new();
    for ext in subkey_names {
        let mut ext_path: Vec<&str> = recent_docs_path.to_vec();
        ext_path.push(ext.as_str());
        docs.extend(parse_recent_docs_extension(hive, &ext_path, &ext, warnings));
    }
    docs
}

fn parse_recent_docs_extension(
    hive: &RegistryHiveReader<'_>,
    ext_path: &[&str],
    ext: &str,
    _warnings: &mut Vec<String>,
) -> Vec<RecentDoc> {
    let ext_nk = match hive.navigate_to(ext_path) {
        Ok(Some(nk)) => nk,
        _ => return Vec::new(),
    };
    let values = match hive.read_all_values_from_nk(&ext_nk) {
        Ok(values) => values,
        _ => return Vec::new(),
    };

    let mut ordered_indices: Vec<u32> = Vec::new();
    for (name, value) in &values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            if let RegistryValue::Binary(data) = value {
                for chunk in data.chunks(4) {
                    if chunk.len() == 4 {
                        let idx = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        if idx == 0xFFFF_FFFF {
                            break;
                        }
                        ordered_indices.push(idx);
                    }
                }
            }
            break;
        }
    }

    let mut entries: Vec<(u32, RecentDoc)> = Vec::new();
    for (name, value) in &values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            continue;
        }
        let Ok(index) = name.parse::<u32>() else {
            continue;
        };
        match value {
            RegistryValue::Binary(data) => {
                if let Some(file_name) = extract_utf16le_from_binary(data) {
                    entries.push((
                        index,
                        RecentDoc {
                            file_name,
                            extension: ext.to_string(),
                            last_accessed: None,
                            lnk_target: None,
                        },
                    ));
                }
            }
            RegistryValue::String(s) => {
                entries.push((
                    index,
                    RecentDoc {
                        file_name: s.clone(),
                        extension: ext.to_string(),
                        last_accessed: None,
                        lnk_target: None,
                    },
                ));
            }
            _ => {}
        }
    }

    if !ordered_indices.is_empty() {
        entries.sort_by_key(|(idx, _)| {
            ordered_indices
                .iter()
                .position(|&i| i == *idx)
                .unwrap_or(usize::MAX)
        });
    } else {
        entries.sort_by_key(|(n, _)| *n);
    }
    entries.into_iter().map(|(_, doc)| doc).collect()
}

// ── UserAssist ───────────────────────────────────────────────────────────────

fn extract_user_assist(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<UserAssistEntry> {
    let ua_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "UserAssist",
    ];
    let ua_nk = match hive.navigate_to(ua_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("UserAssist parse error: {err}"));
            return Vec::new();
        }
    };
    let guid_names = match hive.read_subkey_names_from_nk(&ua_nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("UserAssist GUIDs error: {err}"));
            return Vec::new();
        }
    };
    let mut entries = Vec::new();
    for guid in guid_names {
        let mut count_path: Vec<&str> = ua_path.to_vec();
        count_path.push(guid.as_str());
        count_path.push("Count");
        entries.extend(parse_user_assist_count_key(hive, &count_path, warnings));
    }
    entries
}

fn parse_user_assist_count_key(
    hive: &RegistryHiveReader<'_>,
    count_path: &[&str],
    warnings: &mut Vec<String>,
) -> Vec<UserAssistEntry> {
    let count_nk = match hive.navigate_to(count_path) {
        Ok(Some(nk)) => nk,
        _ => return Vec::new(),
    };
    let values = match hive.read_all_values_from_nk(&count_nk) {
        Ok(values) => values,
        _ => return Vec::new(),
    };
    let mut entries = Vec::new();
    for (name, value) in values {
        if let RegistryValue::Binary(data) = value {
            if data.len() < USER_ASSIST_ENTRY_SIZE {
                warnings.push(format!(
                    "UserAssist entry '{}' binary is {} bytes (expected {USER_ASSIST_ENTRY_SIZE}); skipping",
                    name, data.len()
                ));
                continue;
            }
            let run_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let session_id = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
            let focus_time = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
            let filetime = u64::from_le_bytes([
                data[60], data[61], data[62], data[63], data[64], data[65], data[66], data[67],
            ]);
            let executable_path = rot13_decode(&name);
            let last_run = windows_filetime_to_rfc3339(filetime);
            entries.push(UserAssistEntry {
                executable_path,
                run_count,
                last_run,
                focus_time_ms: focus_time as u64,
                session_id,
            });
        }
    }
    entries
}

// ── TypedURLs (IE) ──────────────────────────────────────────────────────────

fn extract_typed_urls(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let typed_urls_path: &[&str] = &["Software", "Microsoft", "Internet Explorer", "TypedURLs"];
    let nk = match hive.navigate_to(typed_urls_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("TypedURLs parse error: {err}"));
            return Vec::new();
        }
    };
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            warnings.push(format!("TypedURLs values error: {err}"));
            return Vec::new();
        }
    };
    let mut numbered: Vec<(u32, String)> = values
        .into_iter()
        .filter_map(|(name, value)| {
            if let Some(num_str) = name.strip_prefix("url") {
                if let Ok(num) = num_str.parse::<u32>() {
                    if let RegistryValue::String(url) = value {
                        if !url.trim().is_empty() {
                            return Some((num, url));
                        }
                    }
                }
            }
            None
        })
        .collect();
    numbered.sort_by_key(|(n, _)| *n);
    numbered.into_iter().map(|(_, url)| url).collect()
}

// ── WordWheelQuery ──────────────────────────────────────────────────────────

fn extract_word_wheel_query(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let wwq_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "WordWheelQuery",
    ];
    let nk = match hive.navigate_to(wwq_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("WordWheelQuery parse error: {err}"));
            return Vec::new();
        }
    };
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            warnings.push(format!("WordWheelQuery values error: {err}"));
            return Vec::new();
        }
    };

    let mut ordered_indices: Vec<u32> = Vec::new();
    for (name, value) in &values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            if let RegistryValue::Binary(data) = value {
                for chunk in data.chunks(4) {
                    if chunk.len() == 4 {
                        let idx = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        if idx == 0xFFFF_FFFF {
                            break;
                        }
                        ordered_indices.push(idx);
                    }
                }
            }
            break;
        }
    }

    let mut queries: Vec<(u32, String)> = Vec::new();
    for (name, value) in &values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            continue;
        }
        let Ok(index) = name.parse::<u32>() else {
            continue;
        };
        match value {
            RegistryValue::Binary(data) => {
                if let Some(query) = extract_utf16le_from_binary(data) {
                    queries.push((index, query));
                }
            }
            RegistryValue::String(s) if !s.trim().is_empty() => {
                queries.push((index, s.clone()));
            }
            _ => {}
        }
    }
    if !ordered_indices.is_empty() {
        queries.sort_by_key(|(idx, _)| {
            ordered_indices
                .iter()
                .position(|&i| i == *idx)
                .unwrap_or(usize::MAX)
        });
    } else {
        queries.sort_by_key(|(n, _)| *n);
    }
    queries.into_iter().map(|(_, q)| q).collect()
}

// ── OpenSavePidlMRU ─────────────────────────────────────────────────────────

fn extract_open_save_mru(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<OpenSaveMruEntry> {
    let base_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "ComDlg32",
        "OpenSavePidlMRU",
    ];
    let base_nk = match hive.navigate_to(base_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("OpenSavePidlMRU parse error: {err}"));
            return Vec::new();
        }
    };
    let ext_names = match hive.read_subkey_names_from_nk(&base_nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("OpenSavePidlMRU subkeys error: {err}"));
            return Vec::new();
        }
    };

    let mut entries = Vec::new();
    for ext in ext_names {
        let mut ext_path: Vec<&str> = base_path.to_vec();
        ext_path.push(ext.as_str());
        let ext_nk = match hive.navigate_to(&ext_path) {
            Ok(Some(nk)) => nk,
            _ => continue,
        };
        let last_write = ext_nk.last_write_time.and_then(windows_filetime_to_rfc3339);
        let source_key_path = ext_path.join("\\");
        let values = match hive.read_all_values_from_nk(&ext_nk) {
            Ok(values) => values,
            _ => continue,
        };
        for (name, value) in values {
            if name.eq_ignore_ascii_case("MRUListEx") {
                continue;
            }
            if let RegistryValue::Binary(data) = value {
                let file_name = decode_pidl_file_name(&data).unwrap_or_default();
                entries.push(OpenSaveMruEntry {
                    extension: ext.clone(),
                    value_name: name,
                    file_name,
                    raw_pidl_hex: hex::encode(&data),
                    source_key_path: source_key_path.clone(),
                    last_write: last_write.clone(),
                });
            }
        }
    }
    entries
}

// ── LastVisitedPidlMRU ──────────────────────────────────────────────────────

fn extract_last_visited_mru(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<LastVisitedMruEntry> {
    let key_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "ComDlg32",
        "LastVisitedPidlMRU",
    ];
    let nk = match hive.navigate_to(key_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("LastVisitedPidlMRU parse error: {err}"));
            return Vec::new();
        }
    };
    let last_write = nk.last_write_time.and_then(windows_filetime_to_rfc3339);
    let source_key_path = key_path.join("\\");
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            warnings.push(format!("LastVisitedPidlMRU values error: {err}"));
            return Vec::new();
        }
    };

    let mut entries = Vec::new();
    for (name, value) in values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            continue;
        }
        if let RegistryValue::Binary(data) = value {
            let path = decode_pidl_path(&data).unwrap_or_default();
            entries.push(LastVisitedMruEntry {
                value_name: name,
                path,
                raw_pidl_hex: hex::encode(&data),
                source_key_path: source_key_path.clone(),
                last_write: last_write.clone(),
            });
        }
    }
    entries
}

// ── RunMRU ──────────────────────────────────────────────────────────────────

fn extract_run_mru(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<RunMruEntry> {
    let key_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "RunMRU",
    ];
    let nk = match hive.navigate_to(key_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("RunMRU parse error: {err}"));
            return Vec::new();
        }
    };
    let last_write = nk.last_write_time.and_then(windows_filetime_to_rfc3339);
    let source_key_path = key_path.join("\\");
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            warnings.push(format!("RunMRU values error: {err}"));
            return Vec::new();
        }
    };

    values
        .into_iter()
        .filter_map(|(name, value)| {
            if name.eq_ignore_ascii_case("MRUList") || name.eq_ignore_ascii_case("MRUListEx") {
                return None;
            }
            if let RegistryValue::String(command) = value {
                if !command.trim().is_empty() {
                    return Some(RunMruEntry {
                        value_name: name,
                        command,
                        source_key_path: source_key_path.clone(),
                        last_write: last_write.clone(),
                    });
                }
            }
            None
        })
        .collect()
}

/// Returns true for characters that are reasonable inside a decoded file name
/// or path extracted from a PIDL blob. Restricting the set avoids capturing
/// PIDL structural bytes as part of the string.
fn is_reasonable_path_char(c: char) -> bool {
    matches!(
        c,
        'A'..='Z'
            | 'a'..='z'
            | '0'..='9'
            | ' '
            | '.'
            | '_'
            | '-'
            | '~'
            | '\\'
            | '/'
            | ':'
            | '['
            | ']'
            | '('
            | ')'
            | '{'
            | '}'
            | '#'
            | '%'
            | '&'
            | '\''
            | '@'
            | '!'
            | '$'
            | '^'
            | '+'
            | '='
            | ','
            | ';'
    )
}

/// Decode a best-effort file name from a PIDL binary blob.
/// Looks for a UTF-16LE string that starts with a file-name character,
/// contains a period (extension), has only reasonable path characters, and
/// has at least one alphanumeric character, returning the longest match.
fn decode_pidl_file_name(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }
    let mut best: Option<String> = None;
    for start in (0..data.len() - 1).step_by(2) {
        let mut units = Vec::new();
        let mut pos = start;
        while pos + 1 < data.len() {
            let unit = u16::from_le_bytes([data[pos], data[pos + 1]]);
            pos += 2;
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        if units.len() >= 2 {
            let s = String::from_utf16_lossy(&units);
            let starts_ok = s
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '[' || c == '(');
            if starts_ok
                && s.chars().any(|c| c == '.')
                && s.chars().any(|c| c.is_alphanumeric())
                && s.chars().all(is_reasonable_path_char)
                && best.as_ref().is_none_or(|b| s.len() > b.len())
            {
                best = Some(s);
            }
        }
    }
    best
}

/// Decode a best-effort directory path from a PIDL binary blob.
/// Looks for the longest UTF-16LE string that starts with a drive letter or
/// path separator, contains path separators or a drive-letter colon, and only
/// contains reasonable path characters.
pub(crate) fn decode_pidl_path(data: &[u8]) -> Option<String> {
    if data.len() < 6 {
        return None;
    }
    let mut best: Option<String> = None;
    for start in (0..data.len() - 1).step_by(2) {
        let mut units = Vec::new();
        let mut pos = start;
        while pos + 1 < data.len() {
            let unit = u16::from_le_bytes([data[pos], data[pos + 1]]);
            pos += 2;
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        if units.len() >= 3 {
            let s = String::from_utf16_lossy(&units);
            let starts_ok = s
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '\\' || c == '/');
            if starts_ok
                && (s.contains('\\') || s.contains('/') || s.contains(':'))
                && s.chars().all(is_reasonable_path_char)
                && best.as_ref().is_none_or(|b| s.len() > b.len())
            {
                best = Some(s);
            }
        }
    }
    best
}

// ── Default Browser ─────────────────────────────────────────────────────────

fn extract_default_browser(hive: &RegistryHiveReader<'_>) -> Option<String> {
    let path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "Shell",
        "Associations",
        "UrlAssociations",
        "http",
        "UserChoice",
    ];
    let Ok(Some(nk)) = hive.navigate_to(path) else {
        return None;
    };
    let Ok(values) = hive.read_all_values_from_nk(&nk) else {
        return None;
    };
    values
        .into_iter()
        .find_map(|(name, value)| {
            if name.eq_ignore_ascii_case("ProgId") {
                if let RegistryValue::String(prog_id) = value {
                    return Some(prog_id);
                }
            }
            None
        })
        .filter(|s| !s.trim().is_empty())
}

/// Extract program compatibility / elevation flags from
/// `Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers` in
/// an NTUSER.DAT hive.
pub fn extract_appcompat_layers_from_ntuser_hive(
    bytes: &[u8],
    hive_path: &str,
) -> Result<Vec<AppCompatLayerEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows NT",
        "CurrentVersion",
        "AppCompatFlags",
        "Layers",
    ];

    let nk = match hive.navigate_to(key_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };

    let last_write = nk
        .last_write_time
        .and_then(super::windows_filetime_to_rfc3339);
    let source_key_path = key_path.join("\\");
    let values = hive.read_all_values_from_nk(&nk)?;
    let mut entries = Vec::new();

    for (name, value) in values {
        if let RegistryValue::String(layer_string) = value {
            if !name.trim().is_empty() || !layer_string.trim().is_empty() {
                entries.push(AppCompatLayerEntry {
                    executable_path: name,
                    layer_string,
                    source_hive_path: hive_path.to_string(),
                    source_key_path: source_key_path.clone(),
                    last_write: last_write.clone(),
                });
            }
        }
    }

    Ok(entries)
}

// ── MountPoints2 ────────────────────────────────────────────────────────────

fn extract_mount_points(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<MountPoint> {
    let mp_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "MountPoints2",
    ];
    let nk = match hive.navigate_to(mp_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("MountPoints2 parse error: {err}"));
            return Vec::new();
        }
    };
    let subkey_names = match hive.read_subkey_names_from_nk(&nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("MountPoints2 subkeys error: {err}"));
            return Vec::new();
        }
    };
    let mut points = Vec::new();
    for name in subkey_names {
        let mut drive_letter = None;
        let mut volume_guid = None;
        if name.len() == 1 && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            drive_letter = Some(format!("{name}:"));
        } else if name.starts_with('{') && name.ends_with('}') {
            volume_guid = Some(name.clone());
        }
        if drive_letter.is_some() || volume_guid.is_some() {
            points.push(MountPoint {
                drive_letter,
                volume_guid,
                last_mounted: None,
            });
        }
    }
    points
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::test_common::*;
    use super::*;
    use crate::registry::txlog::fixture::{build_synthetic_log1, SyntheticEntry};

    #[test]
    fn test_rot13_decode_basic() {
        assert_eq!(
            rot13_decode("P:\\Jvaqbjf\\Flfgrz32\\abgrcnq.rkr"),
            "C:\\Windows\\System32\\notepad.exe"
        );
        assert_eq!(rot13_decode("Hello"), "Uryyb");
        assert_eq!(rot13_decode("Uryyb"), "Hello");
        assert_eq!(rot13_decode("123"), "123");
        assert_eq!(rot13_decode("!@#"), "!@#");
    }

    #[test]
    fn test_rot13_decode_roundtrip() {
        // ROT13 is its own inverse — decoding twice yields the original.
        let original = "C:\\Users\\Admin\\Desktop\\calc.exe";
        let encoded = rot13_decode(original);
        assert_ne!(original, encoded, "encoded should differ from original");
        assert_eq!(
            rot13_decode(&encoded),
            original,
            "roundtrip should restore original"
        );

        let mixed = "Hello123!@#World";
        assert_eq!(rot13_decode(&rot13_decode(mixed)), mixed);
    }

    #[test]
    fn windows_filetime_converts_to_rfc3339() {
        let ft = 133_600_000_000_000_000u64;
        let ts = windows_filetime_to_rfc3339(ft).expect("valid FILETIME");
        assert!(
            ts.starts_with("2024-") || ts.starts_with("2025-"),
            "timestamp {ts} should be in the 2024-2025 range"
        );
    }

    #[test]
    fn windows_filetime_zero_returns_none() {
        assert_eq!(windows_filetime_to_rfc3339(0), None);
    }

    #[test]
    fn test_empty_userassist_key() {
        let data = empty_hive("NTUSER");
        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert!(info.run_keys.is_empty());
        assert!(info.recent_docs.is_empty());
        assert!(info.ua_entries.is_empty());
        assert!(info.typed_urls.is_empty());
        assert!(info.word_wheel_query.is_empty());
        assert!(info.mount_points.is_empty());
        assert!(info.default_browser.is_none());
    }

    #[test]
    fn extract_ntuser_run_keys() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(&mut data, 0x500, "CurrentVersion", &[("Run", 0x600)], &[]);
        write_nk(&mut data, 0x600, "Run", &[], &[0x700, 0x780]);
        write_string_value(
            &mut data,
            0x700,
            "OneDrive",
            "C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe /background",
            0x1000,
        );
        write_string_value(
            &mut data,
            0x780,
            "SecurityHealth",
            "%ProgramFiles%\\Windows Defender\\MSASCuiL.exe",
            0x1100,
        );

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.run_keys.len(), 2);
        let od = info
            .run_keys
            .iter()
            .find(|k| k.value_name == "OneDrive")
            .unwrap();
        assert!(od.command.contains("OneDrive.exe"));
        assert_eq!(
            od.key_path,
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run"
        );
    }

    #[test]
    fn extract_ntuser_run_once() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("RunOnce", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "RunOnce", &[], &[0x700]);
        write_string_value(
            &mut data,
            0x700,
            "Setup",
            "C:\\Windows\\Setup.exe /silent",
            0x1000,
        );

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.run_keys.len(), 1);
        assert_eq!(info.run_keys[0].value_name, "Setup");
        assert_eq!(
            info.run_keys[0].key_path,
            "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce"
        );
    }

    #[test]
    fn extract_ntuser_recent_docs() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "Explorer", &[("RecentDocs", 0x700)], &[]);
        write_nk(&mut data, 0x700, "RecentDocs", &[(".pdf", 0x800)], &[]);
        write_nk(&mut data, 0x800, ".pdf", &[], &[0x900, 0x980, 0xa00]);

        let mru_list = make_mru_list_ex(&[1, 0]);
        let doc0 = make_recent_doc_binary("report.pdf");
        let doc1 = make_recent_doc_binary("invoice.pdf");

        write_binary_value(&mut data, 0x900, "MRUListEx", &mru_list, 0x1200);
        write_binary_value(&mut data, 0x980, "0", &doc0, 0x1300);
        write_binary_value(&mut data, 0xa00, "1", &doc1, 0x1400);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.recent_docs.len(), 2);
        // MRUListEx [1, 0] means index 1 is most recent
        assert_eq!(info.recent_docs[0].file_name, "invoice.pdf");
        assert_eq!(info.recent_docs[0].extension, ".pdf");
        assert_eq!(info.recent_docs[1].file_name, "report.pdf");
    }

    #[test]
    fn test_userassist_extraction() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "Explorer", &[("UserAssist", 0x700)], &[]);
        write_nk(
            &mut data,
            0x700,
            "UserAssist",
            &[("{CEBFF5CD-ACE2-4F4F-9178-9926F41749EA}", 0x800)],
            &[],
        );
        write_nk(
            &mut data,
            0x800,
            "{CEBFF5CD-ACE2-4F4F-9178-9926F41749EA}",
            &[("Count", 0x900)],
            &[],
        );
        write_nk(&mut data, 0x900, "Count", &[], &[0xa00, 0xb00]);

        let encrypted = "P:\\Jvaqbjf\\Flfgrz32\\abgrcnq.rkr";
        let ft: u64 = 133_600_000_000_000_000;
        // run_count=42, session_id=1, focus_time_ms=1500
        let ua1 = make_user_assist_binary(42, 1, 1500, ft);
        write_binary_value(&mut data, 0xa00, encrypted, &ua1, 0x1200);

        let encrypted2 = "P:\\Hfref\\Grfg\\Qrfxgbc\\pnyp.rkr";
        // run_count=7, session_id=2, focus_time_ms=300
        let ua2 = make_user_assist_binary(7, 2, 300, ft + 86_400_000_000_000);
        write_binary_value(&mut data, 0xb00, encrypted2, &ua2, 0x1300);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.ua_entries.len(), 2);

        let notepad = info
            .ua_entries
            .iter()
            .find(|e| e.executable_path.contains("notepad"))
            .unwrap();
        assert_eq!(notepad.run_count, 42);
        assert_eq!(notepad.session_id, 1);
        assert_eq!(notepad.focus_time_ms, 1500);
        assert!(notepad.last_run.is_some());

        let calc = info
            .ua_entries
            .iter()
            .find(|e| e.executable_path.contains("calc"))
            .unwrap();
        assert_eq!(calc.run_count, 7);
        assert_eq!(calc.session_id, 2);
        assert_eq!(calc.focus_time_ms, 300);
    }

    #[test]
    fn extract_ntuser_typed_urls() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Microsoft",
            &[("Internet Explorer", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Internet Explorer",
            &[("TypedURLs", 0x500)],
            &[],
        );
        write_nk(&mut data, 0x500, "TypedURLs", &[], &[0x600, 0x680, 0x700]);

        write_string_value(
            &mut data,
            0x600,
            "url1",
            "https://forensics.example.com",
            0x1000,
        );
        write_string_value(&mut data, 0x680, "url2", "https://github.com", 0x1100);
        write_string_value(&mut data, 0x700, "url3", "https://www.google.com", 0x1200);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.typed_urls.len(), 3);
        assert_eq!(info.typed_urls[0], "https://forensics.example.com");
        assert_eq!(info.typed_urls[1], "https://github.com");
        assert_eq!(info.typed_urls[2], "https://www.google.com");
    }

    #[test]
    fn extract_ntuser_word_wheel_query() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "Explorer",
            &[("WordWheelQuery", 0x700)],
            &[],
        );
        write_nk(
            &mut data,
            0x700,
            "WordWheelQuery",
            &[],
            &[0x800, 0x880, 0x900],
        );

        let wwq_mru = make_mru_list_ex(&[1, 0]);
        write_binary_value(&mut data, 0x800, "MRUListEx", &wwq_mru, 0x1000);
        write_string_value(&mut data, 0x880, "0", "forensics", 0x1100);
        write_string_value(&mut data, 0x900, "1", "evidence", 0x1200);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.word_wheel_query.len(), 2);
        // MRUListEx [1, 0] -> index 1 is most recent
        assert_eq!(info.word_wheel_query[0], "evidence");
        assert_eq!(info.word_wheel_query[1], "forensics");
    }

    #[test]
    fn extract_ntuser_mount_points() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "Explorer",
            &[("MountPoints2", 0x700)],
            &[],
        );
        write_nk(
            &mut data,
            0x700,
            "MountPoints2",
            &[
                ("C", 0x800),
                ("D", 0x900),
                ("{ecf5d85e-1234-5678-abcd-123456789abc}", 0xa00),
            ],
            &[],
        );
        write_nk(&mut data, 0x800, "C", &[], &[]);
        write_nk(&mut data, 0x900, "D", &[], &[]);
        write_nk(
            &mut data,
            0xa00,
            "{ecf5d85e-1234-5678-abcd-123456789abc}",
            &[],
            &[],
        );

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.mount_points.len(), 3);

        let c = info
            .mount_points
            .iter()
            .find(|m| m.drive_letter.as_deref() == Some("C:"))
            .unwrap();
        assert!(c.volume_guid.is_none());

        let guid = info
            .mount_points
            .iter()
            .find(|m| m.volume_guid.as_deref() == Some("{ecf5d85e-1234-5678-abcd-123456789abc}"))
            .unwrap();
        assert!(guid.drive_letter.is_none());
    }

    #[test]
    fn extract_ntuser_combined() {
        // Run + RecentDocs + UserAssist in one hive.
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x020, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "CurrentVersion",
            &[("Run", 0x700), ("Explorer", 0x800)],
            &[],
        );
        // Run
        write_nk(&mut data, 0x700, "Run", &[], &[0x780]);
        write_string_value(&mut data, 0x780, "OneDrive", "C:\\OneDrive.exe /bg", 0x3000);
        // Explorer
        write_nk(
            &mut data,
            0x800,
            "Explorer",
            &[("RecentDocs", 0x900), ("UserAssist", 0xa00)],
            &[],
        );
        // RecentDocs
        write_nk(&mut data, 0x900, "RecentDocs", &[(".txt", 0xd00)], &[]);
        write_nk(&mut data, 0xd00, ".txt", &[], &[0xd80, 0xdc0]);
        let mru = make_mru_list_ex(&[0]);
        let doc = make_recent_doc_binary("notes.txt");
        write_binary_value(&mut data, 0xd80, "MRUListEx", &mru, 0x3100);
        write_binary_value(&mut data, 0xdc0, "0", &doc, 0x3200);
        // UserAssist
        write_nk(&mut data, 0xa00, "UserAssist", &[("{GUID}", 0xe00)], &[]);
        write_nk(&mut data, 0xe00, "{GUID}", &[("Count", 0xf00)], &[]);
        write_nk(&mut data, 0xf00, "Count", &[], &[0xf80]);
        let ua = make_user_assist_binary(99, 3, 5000, 133_600_000_000_000_000);
        write_binary_value(
            &mut data,
            0xf80,
            "P:\\Hfref\\Grfg\\Qrfxgbc\\pnyp.rkr",
            &ua,
            0x3300,
        );

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.run_keys.len(), 1);
        assert_eq!(info.recent_docs.len(), 1);
        assert_eq!(info.ua_entries.len(), 1);
        assert_eq!(info.run_keys[0].value_name, "OneDrive");
        assert_eq!(info.recent_docs[0].file_name, "notes.txt");
        assert!(info.ua_entries[0].executable_path.contains("calc"));
        assert_eq!(info.ua_entries[0].run_count, 99);
    }

    #[test]
    fn extract_ntuser_combined_group2() {
        // WordWheelQuery + MountPoints2 + TypedURLs in one hive.
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x020, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Microsoft",
            &[("Windows", 0x400), ("Internet Explorer", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "CurrentVersion",
            &[("Explorer", 0x800)],
            &[],
        );
        // Explorer
        write_nk(
            &mut data,
            0x800,
            "Explorer",
            &[("WordWheelQuery", 0x900), ("MountPoints2", 0xa00)],
            &[],
        );
        // WordWheelQuery
        write_nk(&mut data, 0x900, "WordWheelQuery", &[], &[0x980, 0x9c0]);
        let wwq_mru = make_mru_list_ex(&[0]);
        write_string_value(&mut data, 0x980, "0", "search term", 0x3000);
        write_binary_value(&mut data, 0x9c0, "MRUListEx", &wwq_mru, 0x3100);
        // MountPoints2
        write_nk(&mut data, 0xa00, "MountPoints2", &[("E", 0xb00)], &[]);
        write_nk(&mut data, 0xb00, "E", &[], &[]);
        // IE TypedURLs
        write_nk(
            &mut data,
            0x500,
            "Internet Explorer",
            &[("TypedURLs", 0xc00)],
            &[],
        );
        write_nk(&mut data, 0xc00, "TypedURLs", &[], &[0xc80]);
        write_string_value(&mut data, 0xc80, "url1", "https://example.com", 0x3200);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.word_wheel_query.len(), 1);
        assert_eq!(info.mount_points.len(), 1);
        assert_eq!(info.typed_urls.len(), 1);
        assert_eq!(info.word_wheel_query[0], "search term");
        assert_eq!(info.mount_points[0].drive_letter.as_deref(), Some("E:"));
        assert_eq!(info.typed_urls[0], "https://example.com");
    }

    #[test]
    fn extract_ntuser_handles_missing_keys() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Unrelated", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Unrelated", &[], &[]);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert!(info.run_keys.is_empty());
        assert!(info.recent_docs.is_empty());
        assert!(info.ua_entries.is_empty());
        assert!(info.typed_urls.is_empty());
        assert!(info.word_wheel_query.is_empty());
        assert!(info.mount_points.is_empty());
    }

    #[test]
    fn ntuser_hive_with_txlog_overrides_run_key_command() {
        // Build an NTUSER hive with a single Run key.
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(&mut data, 0x500, "CurrentVersion", &[("Run", 0x600)], &[]);
        write_nk(&mut data, 0x600, "Run", &[], &[0x700]);
        write_string_value(&mut data, 0x700, "Malware", "C:\\temp\\old.exe", 0x1000);

        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 200,
            timestamp: Some(0x01DB_A100_0000_0000),
            key_path:
                "\\Registry\\User\\S-1-5-21-123\\Software\\Microsoft\\Windows\\CurrentVersion\\Run"
                    .to_string(),
            value_name: Some("Malware".to_string()),
            data_before: Some(encode_utf16le("C:\\temp\\old.exe")),
            data_after: Some(encode_utf16le("C:\\temp\\new.exe")),
        }]);

        let info =
            extract_ntuser_fields_with_txlog(&data, "Users/Test/NTUSER.DAT", &txlog_bytes).unwrap();

        assert_eq!(info.run_keys.len(), 1);
        assert_eq!(info.run_keys[0].value_name, "Malware");
        assert_eq!(info.run_keys[0].command, "C:\\temp\\new.exe");
        assert!(
            info.run_keys[0].timestamp.is_some(),
            "Run key should have timestamp from txlog"
        );
        assert!(info.txlog_applied);
    }

    #[test]
    fn extract_run_mru_from_fixture() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "Explorer", &[("RunMRU", 0x700)], &[]);
        write_nk(&mut data, 0x700, "RunMRU", &[], &[0x800, 0x880, 0x900]);
        write_string_value(&mut data, 0x800, "MRUList", "acb", 0x4000);
        write_string_value(&mut data, 0x880, "a", "cmd.exe", 0x4100);
        write_string_value(&mut data, 0x900, "b", "powershell.exe", 0x4200);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.run_mru.len(), 2);
        let a = info.run_mru.iter().find(|e| e.value_name == "a").unwrap();
        assert_eq!(a.command, "cmd.exe");
        assert_eq!(
            a.source_key_path,
            "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\RunMRU"
        );
        let b = info.run_mru.iter().find(|e| e.value_name == "b").unwrap();
        assert_eq!(b.command, "powershell.exe");
    }

    #[test]
    fn extract_open_save_mru_from_fixture() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "Explorer", &[("ComDlg32", 0x700)], &[]);
        write_nk(
            &mut data,
            0x700,
            "ComDlg32",
            &[("OpenSavePidlMRU", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "OpenSavePidlMRU", &[("txt", 0x900)], &[]);
        write_nk(&mut data, 0x900, "txt", &[], &[0x980, 0xa00]);
        let mru_list = make_mru_list_ex(&[0]);
        let pidl = make_pidl_blob_with_string("report.txt");
        write_binary_value(&mut data, 0x980, "MRUListEx", &mru_list, 0x4000);
        write_binary_value(&mut data, 0xa00, "0", &pidl, 0x4100);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.open_save_mru.len(), 1);
        let entry = &info.open_save_mru[0];
        assert_eq!(entry.extension, "txt");
        assert_eq!(entry.value_name, "0");
        assert_eq!(entry.file_name, "report.txt");
        assert_eq!(entry.raw_pidl_hex, hex::encode(&pidl));
        assert!(entry.source_key_path.ends_with("OpenSavePidlMRU\\txt"));
    }

    #[test]
    fn extract_last_visited_mru_from_fixture() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "Explorer", &[("ComDlg32", 0x700)], &[]);
        write_nk(
            &mut data,
            0x700,
            "ComDlg32",
            &[("LastVisitedPidlMRU", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "LastVisitedPidlMRU", &[], &[0x880, 0x900]);
        let mru_list = make_mru_list_ex(&[0]);
        let pidl = make_pidl_blob_with_string("C:\\Users\\Test\\Documents");
        write_binary_value(&mut data, 0x880, "MRUListEx", &mru_list, 0x4000);
        write_binary_value(&mut data, 0x900, "0", &pidl, 0x4100);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.last_visited_mru.len(), 1);
        let entry = &info.last_visited_mru[0];
        assert_eq!(entry.value_name, "0");
        assert_eq!(entry.path, "C:\\Users\\Test\\Documents");
        assert_eq!(entry.raw_pidl_hex, hex::encode(&pidl));
        assert!(entry.source_key_path.ends_with("LastVisitedPidlMRU"));
    }

    #[test]
    fn extract_appcompat_layers_from_ntuser_fixture() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows NT", 0xb00)], &[]);
        write_nk(
            &mut data,
            0xb00,
            "Windows NT",
            &[("CurrentVersion", 0xc00)],
            &[],
        );
        write_nk(
            &mut data,
            0xc00,
            "CurrentVersion",
            &[("AppCompatFlags", 0xd00)],
            &[],
        );
        write_nk(
            &mut data,
            0xd00,
            "AppCompatFlags",
            &[("Layers", 0xe00)],
            &[],
        );
        write_nk(&mut data, 0xe00, "Layers", &[], &[0x1500, 0x1580]);
        write_string_value(&mut data, 0x1500, "calc.exe", "WIN7RTM", 0x4000);
        write_string_value(
            &mut data,
            0x1580,
            "C:\\Windows\\System32\\notepad.exe",
            "WINXPSP3 RUNASADMIN",
            0x4100,
        );

        let entries =
            extract_appcompat_layers_from_ntuser_hive(&data, "Users/Test/NTUSER.DAT").unwrap();

        assert_eq!(entries.len(), 2);
        let calc = entries
            .iter()
            .find(|e| e.executable_path == "calc.exe")
            .unwrap();
        assert_eq!(calc.layer_string, "WIN7RTM");
        assert_eq!(
            calc.source_key_path,
            "Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers"
        );
        assert_eq!(calc.source_hive_path, "Users/Test/NTUSER.DAT");

        let notepad = entries
            .iter()
            .find(|e| e.executable_path.contains("notepad.exe"))
            .unwrap();
        assert_eq!(notepad.layer_string, "WINXPSP3 RUNASADMIN");
    }

    fn make_pidl_blob_with_string(s: &str) -> Vec<u8> {
        let mut blob = vec![0x14, 0x00, 0x1f, 0x00, 0xe0, 0x00]; // synthetic PIDL prefix
        let utf16: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        blob.extend_from_slice(&utf16);
        blob.extend_from_slice(&[0x00, 0x00]); // null terminator
        blob.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // trailing padding
        blob
    }
}
