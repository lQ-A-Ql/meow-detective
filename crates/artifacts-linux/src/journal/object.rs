//! Object-level access helpers for the journal object arena.
//!
//! Every object starts with a 16-byte `ObjectHeader` (type, flags, 6 reserved
//! bytes, le64 total size including the header). All object offsets are
//! 8-byte aligned and relative to the start of the file.

/// Object types (`ObjectType` in `journal-def.h`).
pub(super) const OBJECT_DATA: u8 = 1;
pub(super) const OBJECT_ENTRY: u8 = 3;
pub(super) const OBJECT_ENTRY_ARRAY: u8 = 6;

/// `ObjectHeader.flags` compression bits (DATA objects only).
pub(super) const COMPRESSED_XZ: u8 = 1 << 0;
pub(super) const COMPRESSED_LZ4: u8 = 1 << 1;
pub(super) const COMPRESSED_ZSTD: u8 = 1 << 2;

pub(super) const OBJECT_HEADER_LEN: u64 = 16;

#[derive(Debug, Clone, Copy)]
pub(super) struct ObjectHeader {
    pub object_type: u8,
    pub flags: u8,
    /// Total object size in bytes, including the 16-byte object header.
    pub size: u64,
}

/// Read and validate the object header at `offset`. `limit` is the exclusive
/// end of the region objects may occupy (`<= data.len()`). Returns the header
/// and the payload range (after the 16-byte object header) on success.
///
/// All arithmetic is checked; any inconsistency (misalignment, undersized or
/// out-of-bounds object) yields `None` instead of a panic or wrap-around.
pub(super) fn read_object_at(
    data: &[u8],
    offset: u64,
    limit: u64,
) -> Option<(ObjectHeader, &[u8])> {
    if !offset.is_multiple_of(8) || offset.checked_add(OBJECT_HEADER_LEN)? > limit {
        return None;
    }
    let start = usize::try_from(offset).ok()?;
    let raw = data.get(start..start.checked_add(OBJECT_HEADER_LEN as usize)?)?;

    let size = u64::from_le_bytes(raw[8..16].try_into().ok()?);
    if size < OBJECT_HEADER_LEN {
        return None;
    }
    let end = offset.checked_add(size)?;
    if end > limit {
        return None;
    }

    let header = ObjectHeader {
        object_type: raw[0],
        flags: raw[1],
        size,
    };
    let payload = data.get(start + OBJECT_HEADER_LEN as usize..usize::try_from(end).ok()?)?;
    Some((header, payload))
}

/// Advance to the next object in a linear scan: objects are padded to
/// 8-byte boundaries. Returns `None` when no forward progress is possible.
pub(super) fn next_object_offset(offset: u64, size: u64) -> Option<u64> {
    let end = offset.checked_add(size)?;
    Some(end.checked_add(7)? & !7)
}

pub(super) fn read_u64_at(data: &[u8], offset: usize) -> Option<u64> {
    let raw = data.get(offset..offset.checked_add(8)?)?;
    Some(u64::from_le_bytes(raw.try_into().ok()?))
}
