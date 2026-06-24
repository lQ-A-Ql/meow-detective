use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

/// Extract AppCompatCache (ShimCache) entries from the SYSTEM hive.
///
/// The parser is fail-closed: it returns whatever entries it can parse and never
/// panics on an unknown format. It primarily supports the Windows 10/11 format
/// (header magic `0x30` / `0x34`, entry signature `"10ts"`) and falls back to
/// scanning for embedded UTF-16LE paths when the structured parser cannot make
/// progress.
pub fn extract_shimcache_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<ShimCacheEntry>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut warnings = Vec::new();
    let control_sets = hive.control_set_candidates(&mut warnings);
    let mut entries = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for control_set in control_sets {
        let key_path = [
            control_set.as_str(),
            "Control",
            "Session Manager",
            "AppCompatCache",
        ];
        let source_key_path = key_path.join("\\");
        let app_compat = match hive.lookup_value(&key_path, "AppCompatCache") {
            Ok(Some(RegistryValue::Binary(data))) => data,
            Ok(Some(other)) => {
                warnings.push(format!(
                    "{}\\AppCompatCache has unsupported type: {:?}",
                    source_key_path, other
                ));
                continue;
            }
            Ok(None) => {
                warnings.push(format!("{}\\AppCompatCache not found", source_key_path));
                continue;
            }
            Err(err) => {
                warnings.push(format!(
                    "{}\\AppCompatCache parse error: {}",
                    source_key_path, err
                ));
                continue;
            }
        };

        let parsed = parse_shimcache_entries(&app_compat, &source_key_path);
        for entry in parsed {
            if seen_paths.insert(entry.path.clone()) {
                entries.push(entry);
            }
        }
    }

    Ok(entries)
}

fn parse_shimcache_entries(data: &[u8], source_key_path: &str) -> Vec<ShimCacheEntry> {
    const WIN10_MAGIC: &[u8; 4] = b"10ts";
    const WIN8_MAGIC: &[u8; 4] = b"00ts";

    // Known header sizes after which entries begin. Try the most common first.
    let header_candidates = [0x30usize, 0x34, 0x80, 0x14];
    for header_size in header_candidates {
        if data.len() >= header_size + 12 {
            let entries =
                parse_shimcache_entry_stream(&data[header_size..], source_key_path, WIN10_MAGIC);
            if !entries.is_empty() {
                return entries;
            }
            let entries =
                parse_shimcache_entry_stream(&data[header_size..], source_key_path, WIN8_MAGIC);
            if !entries.is_empty() {
                return entries;
            }
        }
    }

    // No structured stream found at a known offset; scan for entry signatures.
    for magic in [WIN10_MAGIC, WIN8_MAGIC] {
        let entries = parse_shimcache_entry_stream(data, source_key_path, magic);
        if !entries.is_empty() {
            return entries;
        }
    }

    // Final fallback: extract any embedded UTF-16LE paths from the blob.
    extract_shimcache_paths_fallback(data, source_key_path)
}

fn parse_shimcache_entry_stream(
    mut data: &[u8],
    source_key_path: &str,
    entry_magic: &[u8; 4],
) -> Vec<ShimCacheEntry> {
    let mut entries = Vec::new();

    while data.len() >= 14 {
        // Find the next entry signature if we are not already aligned on one.
        if &data[..4] != entry_magic {
            if let Some(pos) = data.windows(4).position(|w| w == entry_magic) {
                data = &data[pos..];
            } else {
                break;
            }
        }
        if data.len() < 14 {
            break;
        }

        // Layout for Win10/8.x entries:
        //   0..4   signature
        //   4..8   unknown
        //   8..12  entry length (u32 LE)
        //   12..14 path length (u16 LE)
        //   path   UTF-16LE path
        //   8 bytes FILETIME last modified
        //   2 bytes data length
        //   data_length - 2 bytes data values
        //   2 bytes execution flag
        //   2 bytes padding
        let entry_len = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        if entry_len < 26 || entry_len > data.len() {
            // Skip the signature and continue scanning.
            data = &data[4..];
            continue;
        }
        let path_len = u16::from_le_bytes([data[12], data[13]]) as usize;
        if 14 + path_len + 8 > entry_len {
            data = &data[4..];
            continue;
        }
        let path_bytes = &data[14..14 + path_len];
        let path = decode_shimcache_path(path_bytes);

        let filetime_offset = 14 + path_len;
        let last_modified = if filetime_offset + 8 <= entry_len {
            let filetime = u64::from_le_bytes([
                data[filetime_offset],
                data[filetime_offset + 1],
                data[filetime_offset + 2],
                data[filetime_offset + 3],
                data[filetime_offset + 4],
                data[filetime_offset + 5],
                data[filetime_offset + 6],
                data[filetime_offset + 7],
            ]);
            windows_filetime_to_rfc3339(filetime)
        } else {
            None
        };

        if !path.is_empty() {
            entries.push(ShimCacheEntry {
                path,
                last_modified,
                source_key_path: source_key_path.to_string(),
            });
        }

        data = &data[entry_len..];
    }

    entries
}

fn decode_shimcache_path(bytes: &[u8]) -> String {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return String::new();
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&units);
    s.trim_end_matches('\0').to_string()
}

fn extract_shimcache_paths_fallback(data: &[u8], source_key_path: &str) -> Vec<ShimCacheEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Scan the blob in 2-byte steps looking for plausible UTF-16LE path runs.
    let mut index = 0;
    while index + 2 <= data.len() {
        // A path must contain a backslash and common Windows prefixes.
        let window = &data[index..];
        if let Some(path) = decode_utf16le_path(window) {
            let advance = path.encode_utf16().count() * 2 + 2;
            if path.len() >= 4 && seen.insert(path.clone()) {
                entries.push(ShimCacheEntry {
                    path,
                    last_modified: None,
                    source_key_path: source_key_path.to_string(),
                });
            }
            // Advance by at least the decoded path length in bytes to avoid loops.
            index += advance;
        } else {
            index += 2;
        }
    }

    entries
}

fn decode_utf16le_path(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }
    // Decode until a null unit or non-printable character.
    let mut units = Vec::new();
    for chunk in data.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            break;
        }
        if unit < 0x20 || unit == 0xFFFD {
            return None;
        }
        units.push(unit);
    }
    if units.len() < 4 {
        return None;
    }
    let s = String::from_utf16_lossy(&units);
    let lower = s.to_ascii_lowercase();
    if !lower.contains('\\')
        && !lower.starts_with("c:\\")
        && !lower.starts_with("\\??\\")
        && !lower.starts_with("system32")
        && !lower.starts_with("windows")
    {
        return None;
    }
    Some(s)
}
