// ── UTF-16 decoding helpers ──────────────────────────────────────────────────

use crate::registry::RegistryError;

pub(crate) fn decode_utf16_until_nul(bytes: &[u8]) -> Result<String, RegistryError> {
    let mut decoded = decode_utf16_full(bytes)?;
    if let Some(index) = decoded.find('\0') {
        decoded.truncate(index);
    }
    Ok(decoded)
}

pub(crate) fn decode_utf16_full(bytes: &[u8]) -> Result<String, RegistryError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(RegistryError::utf16("UTF-16 data has odd byte length"));
    }
    let mut units = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        units.push(unit);
    }
    Ok(String::from_utf16_lossy(&units))
}

// ── Byte-reading helpers ──────────────────────────────────────────────────────

pub(crate) fn read_le_array<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    bytes.get(..N)?.try_into().ok()
}
