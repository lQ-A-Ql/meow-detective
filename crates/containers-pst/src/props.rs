//! PST property tags, MAPI types, and property-context parsing.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};

/// Property tag: subject.
pub(crate) const PROP_TAG_SUBJECT: u16 = 0x0037;
/// Property tag: body.
pub(crate) const PROP_TAG_BODY: u16 = 0x1000;
/// Property tag: sender name.
pub(crate) const PROP_TAG_SENDER_NAME: u16 = 0x0C1A;
/// Property tag: sender email address.
pub(crate) const PROP_TAG_SENDER_EMAIL: u16 = 0x0E1F;
/// Property tag: client submit time (sent time).
pub(crate) const PROP_TAG_SENT_TIME: u16 = 0x0039;
/// Property tag: message delivery time (received time).
pub(crate) const PROP_TAG_DELIVERY_TIME: u16 = 0x0E06;
/// Property tag: display to.
pub(crate) const PROP_TAG_DISPLAY_TO: u16 = 0x0E04;
/// Property tag: display cc.
pub(crate) const PROP_TAG_DISPLAY_CC: u16 = 0x0E03;
/// Property tag: message class.
pub(crate) const PROP_TAG_MESSAGE_CLASS: u16 = 0x001A;
/// Property tag: attachment binary data.
pub(crate) const PROP_TAG_ATTACH_DATA: u16 = 0x3701;
/// Property tag: attachment long filename.
pub(crate) const PROP_TAG_ATTACH_LONG_FILENAME: u16 = 0x3707;
/// Property tag: attachment mime type.
pub(crate) const PROP_TAG_ATTACH_MIME: u16 = 0x370E;
/// Property tag: attachment size.
pub(crate) const PROP_TAG_ATTACH_SIZE: u16 = 0x0E20;

/// MAPI property type codes.
#[allow(non_upper_case_globals)]
pub(crate) mod prop_type {
    /// MAPI 16-bit integer property type (format documentation).
    #[allow(dead_code)]
    pub const PtypInteger16: u16 = 0x0002;
    /// MAPI 32-bit integer property type (format documentation).
    #[allow(dead_code)]
    pub const PtypInteger32: u16 = 0x0003;
    /// MAPI 32-bit floating-point property type (format documentation).
    #[allow(dead_code)]
    pub const PtypFloating32: u16 = 0x0004;
    pub const PtypFloating64: u16 = 0x0005;
    /// MAPI boolean property type (format documentation).
    #[allow(dead_code)]
    pub const PtypBoolean: u16 = 0x000B;
    pub const PtypInteger64: u16 = 0x0014;
    pub const PtypString: u16 = 0x001F;
    pub const PtypString8: u16 = 0x001E;
    pub const PtypTime: u16 = 0x0040;
    pub const PtypBinary: u16 = 0x0102;
    /// MAPI multi-valued 16-bit integer type code (format documentation).
    #[allow(dead_code)]
    pub const PtypMultipleInteger16: u16 = 0x1002;
    /// MAPI multi-valued 32-bit integer type code (format documentation).
    #[allow(dead_code)]
    pub const PtypMultipleInteger32: u16 = 0x1003;
    pub const PtypMultipleString: u16 = 0x101F;
    /// MAPI multi-valued binary type code (format documentation).
    #[allow(dead_code)]
    pub const PtypMultipleBinary: u16 = 0x1102;
}

/// Represents a parsed property value.
#[derive(Debug, Clone)]
pub(crate) enum PropValue {
    I64(i64),
    String(String),
    Binary(Vec<u8>),
    Filetime(Option<DateTime<Utc>>),
    StringArray(Vec<String>),
}

pub(crate) fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

pub(crate) fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

pub(crate) fn read_u64_le(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset + 8)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

pub(crate) fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
    if ft == 0 || ft >= 0x8000000000000000 {
        return None;
    }
    let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
    Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32)
        .single()
}

/// Parse a property context (Heap-on-Node) and return all properties.
pub(crate) fn parse_property_context(data: &[u8]) -> BTreeMap<u32, PropValue> {
    let mut props = BTreeMap::new();
    if data.len() < 16 {
        return props;
    }

    // Scan for the "B5 02" or "B5 04" signatures that indicate a property BTH.
    for scan in 0..data.len().saturating_sub(8) {
        if data[scan] != 0xB5 {
            continue;
        }
        let cb_key = data[scan + 1];
        if cb_key != 2 && cb_key != 4 {
            continue;
        }
        let cb_ent = data[scan + 2];
        if cb_ent == 0 {
            continue;
        }
        let hid_root = read_u32_le(data, scan + 4).unwrap_or(0);
        if hid_root == 0 {
            continue;
        }

        // HID: high 5 bits = hidType (0 = HN block), low 27 bits = hidIndex.
        let hid_index = (hid_root & 0x07FF_FFFF) as usize;
        if hid_index >= data.len() {
            continue;
        }

        let val_offset = cb_key as usize;
        let mut idx = hid_index;

        // Limit to a reasonable number of entries.
        for _ in 0..1000 {
            if idx + cb_ent as usize > data.len() {
                break;
            }

            let tag = if cb_key == 2 {
                match read_u16_le(data, idx) {
                    Some(v) => v as u32,
                    None => break,
                }
            } else {
                match read_u32_le(data, idx) {
                    Some(v) => v,
                    None => break,
                }
            };

            let prop_type = (tag & 0xFFFF) as u16;
            let full_tag = tag;

            let val_start = idx + val_offset;
            let val_data = &data[val_start..];

            if let Some(val) = read_prop_value(prop_type, val_data) {
                props.insert(full_tag, val);
            }

            idx += cb_ent as usize;
        }

        break;
    }

    props
}

fn read_prop_value(prop_type: u16, data: &[u8]) -> Option<PropValue> {
    match prop_type {
        prop_type::PtypInteger64 | prop_type::PtypFloating64 => {
            Some(PropValue::I64(read_u64_le(data, 0)? as i64))
        }
        prop_type::PtypString => {
            // Unicode string: length-prefixed in bytes, then UTF-16LE data.
            let len = std::cmp::min(read_u32_le(data, 0)? as usize, data.len().saturating_sub(4));
            let str_data = data.get(4..4 + len)?;
            let chars: Vec<u16> = str_data
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .filter(|&c| c != 0) // skip null terminators
                .collect();
            let s = String::from_utf16(&chars).ok()?;
            Some(PropValue::String(s))
        }
        prop_type::PtypString8 => {
            // ANSI string: length-prefixed, then ASCII/ANSI data.
            let len = std::cmp::min(read_u32_le(data, 0)? as usize, data.len().saturating_sub(4));
            let str_data = data.get(4..4 + len)?;
            let s = String::from_utf8_lossy(str_data)
                .trim_end_matches('\0')
                .to_string();
            Some(PropValue::String(s))
        }
        prop_type::PtypTime => {
            let ft = read_u64_le(data, 0)?;
            Some(PropValue::Filetime(filetime_to_dt(ft)))
        }
        prop_type::PtypBinary => {
            let len = std::cmp::min(read_u32_le(data, 0)? as usize, data.len().saturating_sub(4));
            let bin = data.get(4..4 + len)?.to_vec();
            Some(PropValue::Binary(bin))
        }
        prop_type::PtypMultipleString => {
            // Sequence of null-terminated UTF-16LE strings.
            let mut strings = Vec::new();
            let avail = data.len();
            let mut pos = 0;
            while pos + 2 <= avail {
                let mut end = pos;
                while end + 2 <= avail {
                    let w = u16::from_le_bytes([data[end], data[end + 1]]);
                    if w == 0 {
                        break;
                    }
                    end += 2;
                }
                if end > pos {
                    let chars: Vec<u16> = data[pos..end]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    if let Ok(s) = String::from_utf16(&chars) {
                        if !s.is_empty() {
                            strings.push(s);
                        }
                    }
                }
                pos = end + 2;
                if end + 2 >= avail {
                    break;
                }
            }
            Some(PropValue::StringArray(strings))
        }
        _ => {
            // Unknown type — skip.
            None
        }
    }
}

/// Find a string property value in a property context block.
pub(crate) fn find_prop_string(data: &[u8], prop_id: u16) -> Option<String> {
    let props = parse_property_context(data);
    for full_tag in [
        prop_id as u32,
        ((prop_id as u32) << 16) | prop_type::PtypString as u32,
    ] {
        if let Some(PropValue::String(s)) = props.get(&full_tag) {
            return Some(s.clone());
        }
    }
    // Try with PtypString8 type.
    if let Some(PropValue::String(s)) =
        props.get(&(((prop_id as u32) << 16) | prop_type::PtypString8 as u32))
    {
        return Some(s.clone());
    }
    None
}

/// Find a FILETIME property value.
pub(crate) fn find_prop_filetime(data: &[u8], prop_id: u16) -> Option<DateTime<Utc>> {
    let props = parse_property_context(data);
    let tag = ((prop_id as u32) << 16) | prop_type::PtypTime as u32;
    if let Some(PropValue::Filetime(Some(dt))) = props.get(&tag) {
        return Some(*dt);
    }
    if let Some(PropValue::I64(ft)) = props.get(&((prop_id as u32) << 16)) {
        return filetime_to_dt(*ft as u64);
    }
    None
}

/// Find a multi-valued string property.
pub(crate) fn find_prop_string_array(data: &[u8], prop_id: u16) -> Option<Vec<String>> {
    let props = parse_property_context(data);
    let tag = ((prop_id as u32) << 16) | prop_type::PtypMultipleString as u32;
    if let Some(PropValue::StringArray(arr)) = props.get(&tag) {
        return Some(arr.clone());
    }
    None
}
