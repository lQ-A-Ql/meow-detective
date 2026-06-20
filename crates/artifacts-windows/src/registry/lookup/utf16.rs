// ── UTF-16 decoding helpers ──────────────────────────────────────────────────

pub(crate) fn decode_utf16_until_nul(bytes: &[u8]) -> Result<String, String> {
    let mut decoded = decode_utf16_full(bytes)?;
    if let Some(index) = decoded.find('\0') {
        decoded.truncate(index);
    }
    Ok(decoded)
}

pub(crate) fn decode_utf16_full(bytes: &[u8]) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("UTF-16 data has odd byte length".to_string());
    }
    let mut units = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        units.push(unit);
    }
    Ok(String::from_utf16_lossy(&units))
}

pub(crate) fn decode_name(bytes: &[u8], compressed: bool) -> Result<String, String> {
    if compressed {
        return String::from_utf8(bytes.to_vec()).map_err(|err| err.to_string());
    }
    decode_utf16_full(bytes)
}

// ── Byte-reading helpers ──────────────────────────────────────────────────────

pub(crate) fn read_le_array<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    bytes.get(..N)?.try_into().ok()
}

pub(crate) fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| format!("u16 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid u16".to_string())?,
    ))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| format!("u32 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid u32".to_string())?,
    ))
}

pub(crate) fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, String> {
    Ok(i32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| format!("i32 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid i32".to_string())?,
    ))
}
