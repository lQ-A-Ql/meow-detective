use crate::error::{Result, VolumeAndroidError};

pub(crate) fn read_u16(bytes: &[u8], offset: usize, field: &'static str) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(VolumeAndroidError::Truncated(field))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize, field: &'static str) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(VolumeAndroidError::Truncated(field))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

pub(crate) fn read_u64(bytes: &[u8], offset: usize, field: &'static str) -> Result<u64> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(VolumeAndroidError::Truncated(field))?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

pub(crate) fn read_checksum(bytes: &[u8], offset: usize) -> Result<[u8; 32]> {
    let value = bytes
        .get(offset..offset + 32)
        .ok_or(VolumeAndroidError::Truncated("SHA-256 checksum"))?;
    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(value);
    Ok(checksum)
}

pub(crate) fn read_name(bytes: &[u8], field: &'static str) -> Result<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let value = &bytes[..end];
    if value.is_empty()
        || !value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return Err(VolumeAndroidError::InvalidMetadata(format!(
            "{field} is empty or contains non A-Z/a-z/0-9/_ bytes"
        )));
    }
    String::from_utf8(value.to_vec())
        .map_err(|_| VolumeAndroidError::InvalidMetadata(format!("{field} is not valid ASCII")))
}
