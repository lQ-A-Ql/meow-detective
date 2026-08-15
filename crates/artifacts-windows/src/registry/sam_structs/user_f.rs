use binread::BinRead;

/// SAM per-user `F` value, laid out per the chntpw `sam.h` convention and
/// validated byte-by-byte against a real Windows 11 hive:
///
/// - `0x00` u64 reserved (small constant on every record observed)
/// - `0x08` u64 last logon time
/// - `0x10` u64 account lockout time
/// - `0x18` u64 last password change time
/// - `0x20` u64 account expiry time
/// - `0x28` u64 last failed logon time
/// - `0x30` u32 RID
/// - `0x34` u32 reserved (`0x201` on every record observed)
/// - `0x38` u16 ACB account-control flags
/// - `0x3A` u16 country code
/// - `0x3C` u16 code page
/// - `0x3E` u16 reserved
/// - `0x40` u16 failed logon count
/// - `0x42` u16 total logon count
#[derive(BinRead, Debug)]
#[br(little)]
pub(crate) struct UserFRaw {
    _reserved1: u64,
    pub last_login_time: u64,
    _reserved2: u64,
    pub last_pwd_change_time: u64,
    _reserved3: u64,
    pub _last_failed_login_time: u64,
    pub rid: u32,
    _reserved4: u32,
    pub account_control: u16,
    _reserved5: u32,
    _reserved6: u16,
    pub _failed_login_count: u16,
    pub logon_count: u16,
    _reserved7: [u8; 12],
}

/// Returns `(rid, logon_count, account_control)` for the user.
pub(crate) fn parse_user_f(data: &[u8]) -> Option<(u32, u16, u32)> {
    let mut cursor = std::io::Cursor::new(data);
    let user_f = UserFRaw::read(&mut cursor).ok()?;
    Some((
        user_f.rid,
        user_f.logon_count,
        user_f.account_control as u32,
    ))
}
