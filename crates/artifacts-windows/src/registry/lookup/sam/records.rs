use binread::BinRead;
use std::io::Cursor;

pub(super) fn parse_sam_v_record(
    data: &[u8],
    warnings: &mut Vec<String>,
) -> Option<(u64, u64, u32, u32, u32)> {
    if data.len() < 0x50 {
        warnings.push(format!(
            "SAM V record is {} bytes, expected at least 0x50",
            data.len()
        ));
        return None;
    }
    Some((
        u64::from_le_bytes(data.get(0x08..0x10)?.try_into().ok()?),
        u64::from_le_bytes(data.get(0x18..0x20)?.try_into().ok()?),
        u32::from_le_bytes(data.get(0x28..0x2c)?.try_into().ok()?),
        u32::from_le_bytes(data.get(0x2c..0x30)?.try_into().ok()?),
        data.get(0x46..0x48)
            .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
            .map(u16::from_le_bytes)
            .unwrap_or(0) as u32,
    ))
}

pub(super) fn parse_sam_f_record(
    data: &[u8],
    _warnings: &mut Vec<String>,
) -> Option<(u64, u64, u32, u32)> {
    let mut cursor = Cursor::new(data);
    let user_f = crate::registry::sam_structs::UserFRaw::read(&mut cursor).ok()?;
    Some((
        user_f.last_login_time,
        user_f.last_pwd_change_time,
        user_f.user_attribute,
        user_f.logon_count as u32,
    ))
}
