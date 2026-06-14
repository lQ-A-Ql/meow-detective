//! Apple Property List (plist) parser.
//!
//! Supports two plist formats:
//! - **Binary plist** (`bplist00` magic): parses the trailer, offset table, and
//!   object references to extract key-value pairs.
//! - **XML plist**: parses the XML document to extract `<key>` and value elements.
//!
//! # Binary plist layout (simplified)
//! ```text
//! bplist00           — 8-byte magic
//! <objects>          — variable-length serialized objects
//! <offset table>     — array of byte offsets into object zone
//! <trailer>          — 32-byte fixed trailer with offset table location
//! ```
//!
//! Object marker bits (high nibble):
//! - 0x0: null, 0x0?: boolean, 0x1?: int, 0x2?: real
//! - 0x3?: date, 0x4?: data, 0x5?: ascii string, 0x6?: utf-16 string
//! - 0x8?: uid, 0xA?: array, 0xD?: dict, 0xC?: set
//!
//! # XML plist format
//! ```xml
//! <?xml version="1.0" encoding="UTF-8"?>
//! <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "...">
//! <plist version="1.0">
//! <dict>
//!     <key>CFBundleIdentifier</key>
//!     <string>com.apple.Safari</string>
//! </dict>
//! </plist>
//! ```

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// The type of plist detected from magic bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlistType {
    Binary,
    Xml,
}

/// A single key-value entry extracted from a plist file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MacPlistEntry {
    /// The key name (e.g., "CFBundleIdentifier")
    pub key: String,
    /// The string representation of the value
    pub value: String,
    /// The plist type of the value: "string", "integer", "boolean", "date", "data", "array", "dict"
    #[serde(rename = "type")]
    pub value_type: String,
    /// The file path this entry came from
    pub source_file: String,
}

/// Detect whether raw bytes are a binary plist (`bplist00` magic).
pub fn is_binary_plist(data: &[u8]) -> bool {
    data.len() >= 8 && &data[0..8] == b"bplist00"
}

/// Detect whether raw bytes are an XML plist (starts with XML declaration or <plist>).
pub fn is_xml_plist(data: &[u8]) -> bool {
    let text = std::str::from_utf8(data).unwrap_or("");
    text.trim_start().starts_with("<?xml") || text.contains("<plist")
}

/// Parse a binary plist (`bplist00` format) and return key-value entries.
///
/// This implementation handles the basic binary plist structure:
/// - Reads the 32-byte trailer at the end of the data
/// - Parses the offset table to locate objects
/// - Traverses top-level dict to extract string key → value pairs
pub fn parse_binary_plist(data: &[u8], source_file: &str) -> Result<Vec<MacPlistEntry>, String> {
    if data.len() < 40 {
        return Err("Binary plist too short (minimum 40 bytes required)".to_string());
    }
    if !is_binary_plist(data) {
        return Err("Not a binary plist (missing bplist00 magic)".to_string());
    }

    // Trailer is the last 32 bytes (for files < 16 MB, offset size is 1)
    let trailer_start = data.len().saturating_sub(32);
    let trailer = &data[trailer_start..];
    if trailer.len() < 32 {
        return Err("Binary plist trailer truncated".to_string());
    }

    // Trailer layout (last 32 bytes):
    // [0..5] unused, [5] sort version, [6] offset table int size, [7] object ref size
    // [8..16] num objects (u64 BE), [16..24] top object offset (u64 BE), [24..32] offset table start (u64 BE)
    let offset_size: usize = trailer[6] as usize;
    let obj_ref_size: usize = trailer[7] as usize;
    let num_objects = u64::from_be_bytes([
        trailer[8],
        trailer[9],
        trailer[10],
        trailer[11],
        trailer[12],
        trailer[13],
        trailer[14],
        trailer[15],
    ]) as usize;
    let top_object = u64::from_be_bytes([
        trailer[16],
        trailer[17],
        trailer[18],
        trailer[19],
        trailer[20],
        trailer[21],
        trailer[22],
        trailer[23],
    ]) as usize;
    let offset_table_start = u64::from_be_bytes([
        trailer[24],
        trailer[25],
        trailer[26],
        trailer[27],
        trailer[28],
        trailer[29],
        trailer[30],
        trailer[31],
    ]) as usize;

    if offset_size == 0 || obj_ref_size == 0 || num_objects == 0 {
        return Err("Invalid binary plist trailer values".to_string());
    }

    // Read object offsets from the offset table
    let mut offsets: Vec<usize> = Vec::with_capacity(num_objects);
    for i in 0..num_objects {
        let pos = offset_table_start + i * offset_size;
        if pos + offset_size > data.len() {
            return Err(format!("Offset table entry {} out of bounds", i));
        }
        let off = read_int_be(&data[pos..pos + offset_size]);
        offsets.push(off);
    }

    // The top object is typically a dict (0xD?) — traverse it
    if top_object >= offsets.len() {
        return Err("Top object index out of bounds".to_string());
    }
    let top_offset = offsets[top_object];
    if top_offset + 1 > data.len() {
        return Err("Top object offset out of bounds".to_string());
    }

    let entries = parse_dict(data, top_offset, &offsets, obj_ref_size, source_file)?;
    Ok(entries)
}

/// Parse an XML plist and return key-value entries.
pub fn parse_xml_plist(data: &[u8], source_file: &str) -> Result<Vec<MacPlistEntry>, String> {
    let text = std::str::from_utf8(data).map_err(|_| "XML plist is not valid UTF-8".to_string())?;

    let mut entries: Vec<MacPlistEntry> = Vec::new();
    let mut in_dict = false;
    let mut current_key: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.contains("<dict>") {
            in_dict = true;
            continue;
        }
        if trimmed.contains("</dict>") {
            in_dict = false;
            continue;
        }

        if !in_dict {
            continue;
        }

        // Extract <key>value</key>
        if let Some(key) = extract_xml_content(trimmed, "key") {
            current_key = Some(key);
            continue;
        }

        // Extract value tags
        if let Some(ref key) = current_key {
            for tag in &["string", "integer", "real", "true", "false", "date", "data"] {
                if let Some(value) = extract_xml_content(trimmed, tag) {
                    let value_type = if *tag == "true" || *tag == "false" {
                        "boolean"
                    } else {
                        *tag
                    };
                    let display_value = if *tag == "true" {
                        "true".to_string()
                    } else if *tag == "false" {
                        "false".to_string()
                    } else {
                        value
                    };
                    entries.push(MacPlistEntry {
                        key: key.clone(),
                        value: display_value,
                        value_type: value_type.to_string(),
                        source_file: source_file.to_string(),
                    });
                    current_key = None;
                    break;
                }
            }
        }
    }

    Ok(entries)
}

/// Extract the text content between XML tags like `<tag>content</tag>`.
fn extract_xml_content(line: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let self_close = format!("<{}/>", tag);

    // Self-closing tag
    if line.contains(&self_close) {
        return Some(String::new());
    }

    if let (Some(start), Some(end)) = (line.find(&open), line.find(&close)) {
        let content_start = start + open.len();
        if content_start < end {
            return Some(line[content_start..end].to_string());
        }
        // Empty tag: <string></string>
        return Some(String::new());
    }

    None
}

/// Read a big-endian integer from a byte slice of variable length.
fn read_int_be(bytes: &[u8]) -> usize {
    let mut val: usize = 0;
    for &b in bytes {
        val = (val << 8) | (b as usize);
    }
    val
}

/// Parse a binary plist dict object and return all key-value entries.
fn parse_dict(
    data: &[u8],
    offset: usize,
    offsets: &[usize],
    obj_ref_size: usize,
    source_file: &str,
) -> Result<Vec<MacPlistEntry>, String> {
    if offset >= data.len() {
        return Err("Dict offset out of bounds".to_string());
    }
    let marker = data[offset];
    let marker_type = marker >> 4;
    if marker_type != 0xD {
        return Err(format!(
            "Expected dict marker (0xDx) at offset {}, got 0x{:02X}",
            offset, marker
        ));
    }
    let count = (marker & 0x0F) as usize;

    // Each dict entry is two obj_ref_size integers: key ref, value ref
    let entry_size = obj_ref_size * 2;
    let dict_data_start = offset + 1;

    let mut entries: Vec<MacPlistEntry> = Vec::new();
    for i in 0..count {
        let entry_offset = dict_data_start + i * entry_size;
        let key_ref = read_int_be(&data[entry_offset..entry_offset + obj_ref_size]);
        let val_ref = read_int_be(&data[entry_offset + obj_ref_size..entry_offset + entry_size]);

        let key_str = read_string_object(data, key_ref, offsets)?;
        let (val_str, val_type) = read_value_string(data, val_ref, offsets)?;

        entries.push(MacPlistEntry {
            key: key_str,
            value: val_str,
            value_type: val_type,
            source_file: source_file.to_string(),
        });
    }

    Ok(entries)
}

/// Read a binary plist string object (0x5x ASCII, 0x6x UTF-16).
fn read_string_object(data: &[u8], ref_idx: usize, offsets: &[usize]) -> Result<String, String> {
    if ref_idx >= offsets.len() {
        return Ok("<invalid ref>".to_string());
    }
    let offset = offsets[ref_idx];
    if offset >= data.len() {
        return Ok("<out of bounds>".to_string());
    }
    let marker = data[offset];
    let marker_type = marker >> 4;
    let len = (marker & 0x0F) as usize;

    match marker_type {
        0x5 => {
            // ASCII string
            let start = offset + 1;
            let end = std::cmp::min(start + len, data.len());
            Ok(String::from_utf8_lossy(&data[start..end]).to_string())
        }
        0x6 => {
            // UTF-16BE string
            let start = offset + 1;
            let byte_len = len * 2;
            let end = std::cmp::min(start + byte_len, data.len());
            let u16s: Vec<u16> = data[start..end]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            Ok(String::from_utf16(&u16s).unwrap_or_else(|_| "<invalid utf-16>".to_string()))
        }
        _ => Ok(format!("<non-string ref type 0x{:X}>", marker_type)),
    }
}

/// Read a binary plist value and return (string_value, type_name).
fn read_value_string(
    data: &[u8],
    ref_idx: usize,
    offsets: &[usize],
) -> Result<(String, String), String> {
    if ref_idx >= offsets.len() {
        return Ok(("<invalid ref>".to_string(), "unknown".to_string()));
    }
    let offset = offsets[ref_idx];
    if offset >= data.len() {
        return Ok(("<out of bounds>".to_string(), "unknown".to_string()));
    }
    let marker = data[offset];
    let marker_type = marker >> 4;
    let extra = (marker & 0x0F) as usize;

    match marker_type {
        0x0 if extra == 0 => Ok(("null".to_string(), "null".to_string())),
        0x0 => {
            // Boolean: 0x08 = false, 0x09 = true
            if extra == 0x9 {
                Ok(("true".to_string(), "boolean".to_string()))
            } else {
                Ok(("false".to_string(), "boolean".to_string()))
            }
        }
        0x1 => {
            // Integer
            let byte_count = 1 << extra;
            let start = offset + 1;
            let end = std::cmp::min(start + byte_count, data.len());
            let val = read_int_be(&data[start..end]);
            Ok((val.to_string(), "integer".to_string()))
        }
        0x2 => {
            // Real (float)
            let start = offset + 1;
            let end = std::cmp::min(start + (extra + 1), data.len());
            let float_val = if extra == 2 {
                // 4-byte float
                if end - start >= 4 {
                    f64::from(f32::from_be_bytes([
                        data[start],
                        data[start + 1],
                        data[start + 2],
                        data[start + 3],
                    ]))
                } else {
                    0.0
                }
            } else if extra == 3 {
                // 8-byte double
                if end - start >= 8 {
                    f64::from_be_bytes([
                        data[start],
                        data[start + 1],
                        data[start + 2],
                        data[start + 3],
                        data[start + 4],
                        data[start + 5],
                        data[start + 6],
                        data[start + 7],
                    ])
                } else {
                    0.0
                }
            } else {
                0.0
            };
            Ok((format!("{}", float_val), "real".to_string()))
        }
        0x3 => {
            // Date: 8-byte float seconds since 2001-01-01
            let start = offset + 1;
            let end = std::cmp::min(start + 8, data.len());
            if end - start >= 8 {
                let secs = f64::from_be_bytes([
                    data[start],
                    data[start + 1],
                    data[start + 2],
                    data[start + 3],
                    data[start + 4],
                    data[start + 5],
                    data[start + 6],
                    data[start + 7],
                ]);
                // Apple epoch: 2001-01-01T00:00:00Z = 978307200 seconds since Unix epoch
                let unix_secs = secs + 978_307_200.0;
                if unix_secs >= 0.0 && unix_secs < (i64::MAX as f64) {
                    let dt = Utc
                        .timestamp_opt(
                            unix_secs as i64,
                            ((unix_secs - unix_secs.floor()) * 1_000_000_000.0) as u32,
                        )
                        .single();
                    match dt {
                        Some(d) => Ok((d.to_rfc3339(), "date".to_string())),
                        None => Ok((format!("+{:.6}s (apple epoch)", secs), "date".to_string())),
                    }
                } else {
                    Ok((format!("+{:.6}s (apple epoch)", secs), "date".to_string()))
                }
            } else {
                Ok(("<truncated date>".to_string(), "date".to_string()))
            }
        }
        0x4 => {
            // Data (binary)
            let len = extra;
            let start = offset + 1;
            let end = std::cmp::min(start + len, data.len());
            let hex: String = data[start..end]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect();
            Ok((format!("<{} bytes: {}>", len, hex), "data".to_string()))
        }
        0x5 => {
            // ASCII string (reuse read_string_object logic)
            let s = read_string_object(data, ref_idx, offsets)
                .unwrap_or_else(|_| "<error>".to_string());
            Ok((s, "string".to_string()))
        }
        0x6 => {
            // UTF-16 string
            let s = read_string_object(data, ref_idx, offsets)
                .unwrap_or_else(|_| "<error>".to_string());
            Ok((s, "string".to_string()))
        }
        0x8 => {
            // UID
            let start = offset + 1;
            let byte_count = extra + 1;
            let end = std::cmp::min(start + byte_count, data.len());
            let uid = read_int_be(&data[start..end]);
            Ok((format!("uid:{}", uid), "uid".to_string()))
        }
        0xA => Ok(("<array>".to_string(), "array".to_string())),
        0xC => Ok(("<set>".to_string(), "set".to_string())),
        0xD => Ok(("<dict>".to_string(), "dict".to_string())),
        _ => Ok((
            format!("<unknown type 0x{:X}>", marker_type),
            "unknown".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid binary plist with one string key-value pair.
    fn build_minimal_bplist() -> Vec<u8> {
        let mut buf = Vec::new();

        // Magic header
        buf.extend_from_slice(b"bplist00"); // 0..8

        // obj0 at 8: key "CFBundleName" -> marker 0x5C + 12 bytes
        buf.push(0x5C);
        buf.extend_from_slice(b"CFBundleName");
        let obj0_off = 8u32;

        // obj1 at 21: val "Safari" -> marker 0x56 + 6 bytes
        buf.push(0x56);
        buf.extend_from_slice(b"Safari");
        let obj1_off = obj0_off + 13;

        // obj2 at 28: dict 1 entry -> marker 0xD1 + key_ref(0) + val_ref(1) (4-byte refs)
        buf.push(0xD1);
        buf.extend_from_slice(&0u32.to_be_bytes()); // key ref = obj 0
        buf.extend_from_slice(&1u32.to_be_bytes()); // val ref = obj 1
        let obj2_off = obj1_off + 7;

        let num_objects: u32 = 3;

        // Offset table
        let ot_actual_start = buf.len() as u32;
        buf.extend_from_slice(&obj0_off.to_be_bytes());
        buf.extend_from_slice(&obj1_off.to_be_bytes());
        buf.extend_from_slice(&obj2_off.to_be_bytes());

        // Trailer (32 bytes)
        buf.extend_from_slice(&[0u8; 5]); // unused
        buf.push(0); // sort version
        buf.push(4); // offset int size
        buf.push(4); // object ref size
        buf.extend_from_slice(&(num_objects as u64).to_be_bytes()); // num objects
        buf.extend_from_slice(&(2u64).to_be_bytes()); // top object index (obj2)
        buf.extend_from_slice(&(ot_actual_start as u64).to_be_bytes()); // offset table start

        buf
    }

    #[test]
    fn detect_binary_magic() {
        let data = b"bplist00.......";
        assert!(is_binary_plist(data));
        assert!(!is_binary_plist(b"notaplist"));
    }

    #[test]
    fn detect_xml_plist() {
        let data = b"<?xml version=\"1.0\"?>\n<plist version=\"1.0\">";
        assert!(is_xml_plist(data));
        assert!(!is_xml_plist(b"not an xml plist"));
    }

    #[test]
    fn parse_bplist_minimal() {
        let data = build_minimal_bplist();
        let entries = parse_binary_plist(&data, "/test/Info.plist").expect("should parse");
        assert!(!entries.is_empty(), "Expected at least one entry");
        // Find our key
        let found = entries.iter().find(|e| e.key == "CFBundleName");
        assert!(found.is_some(), "Should find CFBundleName key");
        assert_eq!(found.unwrap().value, "Safari");
        assert_eq!(found.unwrap().value_type, "string");
    }

    #[test]
    fn parse_bplist_rejects_non_bplist() {
        let result = parse_binary_plist(b"not a plist...", "test");
        assert!(result.is_err());
    }

    #[test]
    fn parse_bplist_rejects_short_data() {
        let result = parse_binary_plist(b"bplist00", "test");
        assert!(result.is_err());
    }

    #[test]
    fn parse_xml_plist_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.apple.Safari</string>
    <key>CFBundleVersion</key>
    <string>19618.1.15</string>
    <key>LSRequiresIPhoneOS</key>
    <true/>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
</dict>
</plist>"#;

        let entries = parse_xml_plist(xml.as_bytes(), "/test/Info.plist").expect("should parse");
        assert_eq!(entries.len(), 4);

        let id = entries
            .iter()
            .find(|e| e.key == "CFBundleIdentifier")
            .unwrap();
        assert_eq!(id.value, "com.apple.Safari");
        assert_eq!(id.value_type, "string");

        let iphone = entries
            .iter()
            .find(|e| e.key == "LSRequiresIPhoneOS")
            .unwrap();
        assert_eq!(iphone.value, "true");
        assert_eq!(iphone.value_type, "boolean");
    }

    #[test]
    fn parse_xml_plist_with_integer_and_boolean() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>MaxConnections</key>
    <integer>42</integer>
    <key>IsEnabled</key>
    <false/>
    <key>Version</key>
    <real>3.14</real>
</dict>
</plist>"#;

        let entries = parse_xml_plist(xml.as_bytes(), "test.plist").expect("should parse");
        assert_eq!(entries.len(), 3);

        let max = entries.iter().find(|e| e.key == "MaxConnections").unwrap();
        assert_eq!(max.value, "42");
        assert_eq!(max.value_type, "integer");

        let enabled = entries.iter().find(|e| e.key == "IsEnabled").unwrap();
        assert_eq!(enabled.value, "false");
        assert_eq!(enabled.value_type, "boolean");

        let ver = entries.iter().find(|e| e.key == "Version").unwrap();
        assert_eq!(ver.value, "3.14");
        assert_eq!(ver.value_type, "real");
    }

    #[test]
    fn parse_xml_plist_empty() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
</dict>
</plist>"#;
        let entries = parse_xml_plist(xml.as_bytes(), "empty.plist").expect("should parse");
        assert!(entries.is_empty());
    }
}
