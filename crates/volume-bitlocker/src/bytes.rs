//! Bounds-checked little-endian readers over untrusted volume bytes.
//!
//! Derived from `bitlocker-core`'s `bytes` module (see `../NOTICE`).
//!
//! Every multi-byte read goes through these helpers. An out-of-range offset
//! yields a zero value rather than panicking, because the input is an
//! attacker-controllable BitLocker volume: a lying length field must produce a
//! parse failure further up, never an index panic in the evidence reader.
//! BitLocker structures are little-endian throughout.

/// Reads a little-endian `u16` at `offset`, yielding 0 when out of range.
pub(crate) fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    let mut buf = [0u8; 2];
    if let Some(slice) = bytes.get(offset..offset.saturating_add(2)) {
        buf.copy_from_slice(slice);
    }
    u16::from_le_bytes(buf)
}

/// Reads a little-endian `u32` at `offset`, yielding 0 when out of range.
pub(crate) fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    let mut buf = [0u8; 4];
    if let Some(slice) = bytes.get(offset..offset.saturating_add(4)) {
        buf.copy_from_slice(slice);
    }
    u32::from_le_bytes(buf)
}

/// Reads a little-endian `u64` at `offset`, yielding 0 when out of range.
pub(crate) fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut buf = [0u8; 8];
    if let Some(slice) = bytes.get(offset..offset.saturating_add(8)) {
        buf.copy_from_slice(slice);
    }
    u64::from_le_bytes(buf)
}

/// Reads a 16-byte GUID at `offset`, yielding all zeros when out of range.
pub(crate) fn read_guid(bytes: &[u8], offset: usize) -> [u8; 16] {
    let mut guid = [0u8; 16];
    if let Some(slice) = bytes.get(offset..offset.saturating_add(16)) {
        guid.copy_from_slice(slice);
    }
    guid
}

/// Copies `[offset, offset + len)`, truncated to what is actually present.
///
/// An out-of-range start yields an empty vector rather than an error, so a
/// truncated metadata entry degrades to "no value" instead of aborting the
/// whole block parse — the remaining entries may still be readable.
pub(crate) fn slice_owned(bytes: &[u8], offset: usize, len: usize) -> Vec<u8> {
    let end = offset.saturating_add(len).min(bytes.len());
    bytes
        .get(offset..end)
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "../tests/unit/bytes.rs"]
mod tests;
