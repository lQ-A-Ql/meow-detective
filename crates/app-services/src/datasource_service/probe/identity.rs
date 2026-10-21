use super::super::{ImageFilesystemKind, Result};
use std::io::{Read, Seek, SeekFrom};

pub(crate) fn read_filesystem_uuid<R: Read + Seek + ?Sized>(
    reader: &mut R,
    partition_offset: u64,
    partition_length: Option<u64>,
    kind: ImageFilesystemKind,
) -> Result<Option<String>> {
    let (offset, length, magic_offset, magic) = match kind {
        ImageFilesystemKind::Ext4 => (1024u64, 120usize, 56usize, &[0x53, 0xef][..]),
        ImageFilesystemKind::Xfs => (0, 48, 0, b"XFSB" as &[u8]),
        ImageFilesystemKind::Btrfs => (0x10000, 0x48, 0x40, b"_BHRfS_M" as &[u8]),
        _ => return Ok(None),
    };
    if partition_length.is_some_and(|length_bytes| {
        offset
            .checked_add(length as u64)
            .is_none_or(|required_bytes| required_bytes > length_bytes)
    }) {
        return Ok(None);
    }
    let absolute = partition_offset.checked_add(offset).ok_or_else(|| {
        super::super::DataSourceError::Evidence("filesystem UUID offset overflow".into())
    })?;
    reader.seek(SeekFrom::Start(absolute))?;
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    if bytes.get(magic_offset..magic_offset + magic.len()) != Some(magic) {
        return Ok(None);
    }
    let uuid_offset = match kind {
        ImageFilesystemKind::Ext4 => 104,
        ImageFilesystemKind::Xfs => 32,
        ImageFilesystemKind::Btrfs => 32,
        _ => return Ok(None),
    };
    let uuid: [u8; 16] = bytes
        .get(uuid_offset..uuid_offset + 16)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| {
            super::super::DataSourceError::Evidence("filesystem UUID is truncated".into())
        })?;
    Ok((uuid != [0; 16]).then(|| uuid::Uuid::from_bytes(uuid).to_string()))
}
