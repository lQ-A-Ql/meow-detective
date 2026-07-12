use binread::BinRead;

#[derive(BinRead, Debug)]
#[br(little)]
pub struct UserFRaw {
    _unknown1: u64,
    pub last_login_time: u64,
    _unknown2: u64,
    pub last_pwd_change_time: u64,
    _unknown3: u64,
    pub last_failed_login_time: u64,
    pub rid: u32,
    pub user_attribute: u32,
    pub logon_count: u16,
    pub invalid_login_count: u16,
    _unknown4: [u8; 20],
}

pub fn parse_user_f(data: &[u8]) -> Option<(u32, u16, u32)> {
    let mut cursor = std::io::Cursor::new(data);
    let user_f = UserFRaw::read(&mut cursor).ok()?;
    Some((user_f.rid, user_f.logon_count, user_f.user_attribute))
}
