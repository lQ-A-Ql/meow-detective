pub(super) fn type_name(code: u32) -> String {
    let name = match code {
        0 => "REG_NONE",
        1 => "REG_SZ",
        2 => "REG_EXPAND_SZ",
        3 => "REG_BINARY",
        4 => "REG_DWORD",
        5 => "REG_DWORD_BIG_ENDIAN",
        6 => "REG_LINK",
        7 => "REG_MULTI_SZ",
        8 => "REG_RESOURCE_LIST",
        9 => "REG_FULL_RESOURCE_DESCRIPTOR",
        10 => "REG_RESOURCE_REQUIREMENTS_LIST",
        11 => "REG_QWORD",
        _ => return format!("REG_UNKNOWN_{code}"),
    };
    name.to_string()
}

pub(super) fn decode(code: u32, bytes: &[u8], offset: usize, partial: bool) -> Option<String> {
    match code {
        1 | 2 | 6 | 7 if offset.is_multiple_of(2) => {
            if !bytes.len().is_multiple_of(2) && !partial {
                return None;
            }
            let units: Vec<_> = bytes
                .chunks_exact(2)
                .map(|v| u16::from_le_bytes([v[0], v[1]]))
                .collect();
            let mut text = if partial {
                String::from_utf16_lossy(&units)
            } else {
                String::from_utf16(&units).ok()?
            };
            if !partial {
                text = text.trim_end_matches('\0').to_string();
            }
            if code == 7 {
                text = text.replace('\0', "\n");
            }
            Some(text)
        }
        4 if offset == 0 && !partial && bytes.len() == 4 => {
            Some(u32::from_le_bytes(bytes.try_into().ok()?).to_string())
        }
        5 if offset == 0 && !partial && bytes.len() == 4 => {
            Some(u32::from_be_bytes(bytes.try_into().ok()?).to_string())
        }
        11 if offset == 0 && !partial && bytes.len() == 8 => {
            Some(u64::from_le_bytes(bytes.try_into().ok()?).to_string())
        }
        _ => None,
    }
}
