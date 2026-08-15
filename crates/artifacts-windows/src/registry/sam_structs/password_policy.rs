use binread::BinRead;

#[derive(BinRead, Debug)]
#[br(little)]
pub(crate) struct DomainAccountFRaw {
    pub _revision: u32,
    _pad1: u32,
    pub _creation_time: u64,
    pub _domain_modified_count: u64,
    pub max_pwd_age: u64,
    pub min_pwd_age: u64,
    pub _force_logoff: u64,
    pub lockout_duration: u64,
    pub lockout_observation_window: u64,
    _pad2: u64,
    pub _next_rid: u32,
    pub _pwd_properties: u32,
    pub min_pwd_length: u16,
    pub pwd_history_length: u16,
    pub lockout_threshold: u16,
    _pad3: u16,
    pub _server_state: u32,
    pub _server_role: u16,
    pub _uas_compatibility_req: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SamPasswordPolicy {
    pub max_password_age_days: u64,
    pub min_password_age_days: u64,
    pub min_password_length: u16,
    pub password_history_length: u16,
    pub lockout_threshold: u16,
    pub lockout_duration_minutes: u64,
    pub lockout_observation_window_minutes: u64,
}

const HUNDRED_NS_PER_DAY: u64 = 864_000_000_000;
const HUNDRED_NS_PER_MINUTE: u64 = 600_000_000;

pub(crate) fn parse_domain_account_f(data: &[u8]) -> Option<SamPasswordPolicy> {
    let mut cursor = std::io::Cursor::new(data);
    let raw = DomainAccountFRaw::read(&mut cursor).ok()?;
    Some(SamPasswordPolicy {
        max_password_age_days: duration_units(raw.max_pwd_age, HUNDRED_NS_PER_DAY),
        min_password_age_days: duration_units(raw.min_pwd_age, HUNDRED_NS_PER_DAY),
        min_password_length: raw.min_pwd_length,
        password_history_length: raw.pwd_history_length,
        lockout_threshold: raw.lockout_threshold,
        lockout_duration_minutes: duration_units(raw.lockout_duration, HUNDRED_NS_PER_MINUTE),
        lockout_observation_window_minutes: duration_units(
            raw.lockout_observation_window,
            HUNDRED_NS_PER_MINUTE,
        ),
    })
}

fn duration_units(ticks: u64, unit: u64) -> u64 {
    if ticks == 0 {
        0
    } else {
        ticks / unit
    }
}
