use uuid::Uuid;

/// Canonical identity shared by STORAGE volumes, MountedDevices and WPD.
/// Instance components are compared whole; a serial substring is never enough.
pub(crate) fn volume_identity(value: &str) -> Option<String> {
    let value = value.trim_end_matches('\0').to_ascii_lowercase();
    let value = value.strip_prefix("swd#wpdbusenum#").unwrap_or(&value);
    let value = value
        .strip_prefix("_??_")
        .or_else(|| value.strip_prefix(r"\??\"))
        .or_else(|| value.strip_prefix(r"\\?\"))
        .unwrap_or(value);
    let parts: Vec<_> = value.split('#').collect();
    if parts.len() == 2 && parts[0].starts_with('{') {
        let disk = Uuid::parse_str(parts[0]).ok()?;
        if disk.is_nil() || parts[1].len() != 16 || !parts[1].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return None;
        }
        return Some(format!("{{{disk}}}#{}", parts[1]));
    }
    if (parts.len() == 3 || parts.len() == 4)
        && matches!(parts[0], "usbstor" | "usb")
        && !parts[1].is_empty()
        && !parts[2].is_empty()
        && (parts.len() == 3 || Uuid::parse_str(parts[3]).is_ok())
    {
        return Some(parts[..3].join("\\"));
    }
    None
}

pub(crate) fn utf16_text(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<_> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|c| *c != 0)
        .collect();
    String::from_utf16(&units).ok()
}
