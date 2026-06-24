use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

// ── Run / RunOnce ────────────────────────────────────────────────────────────

pub(super) fn extract_run_keys(
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

pub(super) fn extract_recent_docs(
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

// ── OpenSavePidlMRU ─────────────────────────────────────────────────────────

pub(super) fn extract_open_save_mru(
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

pub(super) fn extract_last_visited_mru(
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

pub(super) fn extract_run_mru(
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

// ── PIDL decode helpers ─────────────────────────────────────────────────────

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
