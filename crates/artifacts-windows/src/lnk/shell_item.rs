/// Parse shell items from a LinkTargetIDList.
/// Each shell item: 2 bytes size, 1 byte type, variable data.
/// Gathers file/directory names to build a partial path.
pub fn parse_shell_items(data: &[u8]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut pos = 0usize;
    while pos + 3 <= data.len() {
        let item_size = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        if item_size < 3 || pos + item_size > data.len() {
            break;
        }
        let item_type = data[pos + 2];
        let item_data = &data[pos + 3..pos + item_size];

        match item_type {
            0x1F | 0x23 => {
                // Root folder / drive — skip GUID/volume info
            }
            0x2F | 0x30 | 0x31 | 0x32 | 0x35 => {
                // File/directory entry — extract name
                if let Some(name) = extract_shell_item_name(item_data, item_type) {
                    parts.push(name);
                }
            }
            _ => {}
        }
        pos += item_size;
    }

    parts.join("\\")
}

fn extract_shell_item_name(data: &[u8], item_type: u8) -> Option<String> {
    match item_type {
        0x2F | 0x30 => {
            // Extension block offset at byte 12 varies, simple version:
            // Name is ASCII/UTF-16LE at a fixed offset depending on flags
            if data.len() > 14 {
                let flags = data[14];
                let name_offset = if flags & 0x04 != 0 { 0x14 } else { 0x0E };
                let is_unicode = data.get(15).copied().unwrap_or(0) & 0x80 != 0;
                read_name(data, name_offset, is_unicode)
            } else {
                None
            }
        }
        0x31 | 0x32 => {
            // Short name only: read from offset 0x04 or 0x06
            if data.len() > 6 {
                read_name(data, 4, false)
            } else {
                None
            }
        }
        0x35 => {
            // Extension block: name at offset 0x0C
            if data.len() > 12 {
                read_name(data, 12, false)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn read_name(data: &[u8], offset: usize, is_unicode: bool) -> Option<String> {
    if offset >= data.len() {
        return None;
    }
    let remainder = &data[offset..];
    if is_unicode {
        let mut chars = Vec::new();
        for chunk in remainder.chunks_exact(2) {
            let c = u16::from_le_bytes([chunk[0], chunk[1]]);
            if c == 0 {
                break;
            }
            chars.push(c);
        }
        Some(String::from_utf16_lossy(&chars))
    } else {
        let end = remainder
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(remainder.len());
        Some(String::from_utf8_lossy(&remainder[..end]).into_owned())
    }
}
