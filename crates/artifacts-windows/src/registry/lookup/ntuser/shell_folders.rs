use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

// ── TypedURLs (IE) ──────────────────────────────────────────────────────────

pub(super) fn extract_typed_urls(
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

pub(super) fn extract_word_wheel_query(
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

// ── Default Browser ─────────────────────────────────────────────────────────

pub(super) fn extract_default_browser(hive: &RegistryHiveReader<'_>) -> Option<String> {
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
