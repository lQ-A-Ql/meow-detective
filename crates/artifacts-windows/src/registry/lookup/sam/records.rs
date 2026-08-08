use binread::BinRead;
use std::io::Cursor;

pub(super) fn parse_sam_f_record(
    data: &[u8],
    _warnings: &mut Vec<String>,
) -> Option<(u64, u64, u32, u32)> {
    let mut cursor = Cursor::new(data);
    let user_f = crate::registry::sam_structs::UserFRaw::read(&mut cursor).ok()?;
    Some((
        user_f.last_login_time,
        user_f.last_pwd_change_time,
        user_f.account_control as u32,
        user_f.logon_count as u32,
    ))
}
