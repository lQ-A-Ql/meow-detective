use super::TxlogTimestampInfo;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamUser {
    pub username: String,
    pub rid: u32,
    pub sid: String,
    pub full_name: String,
    pub comment: String,
    pub home_dir: String,
    pub profile_path: String,
    pub last_login: Option<DateTime<Utc>>,
    pub password_last_set: Option<DateTime<Utc>>,
    pub account_disabled: bool,
    pub account_locked: bool,
    pub admin_count: u32,
    pub login_count: u32,
    pub group_memberships: Vec<String>,
    pub password_hash: Option<String>,
    pub password_hash_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamGroup {
    pub name: String,
    pub rid: u32,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SamInfo {
    pub users: Vec<SamUser>,
    pub groups: Vec<SamGroup>,
    pub password_policy: Option<crate::registry::sam_structs::SamPasswordPolicy>,
    pub txlog_applied: bool,
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}
