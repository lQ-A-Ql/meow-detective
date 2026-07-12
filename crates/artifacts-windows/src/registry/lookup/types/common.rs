use chrono::{DateTime, Utc};

pub(crate) const BASE_BLOCK_SIZE: usize = 0x1000;
pub(crate) const NK_SIGNATURE: &[u8; 2] = b"nk";
pub(crate) const VK_SIGNATURE: &[u8; 2] = b"vk";
pub(crate) const REG_SZ: u32 = 1;
pub(crate) const REG_EXPAND_SZ: u32 = 2;
pub(crate) const REG_DWORD: u32 = 4;
pub(crate) const REG_MULTI_SZ: u32 = 7;
pub(crate) const REG_QWORD: u32 = 11;
pub(crate) const INVALID_OFFSET: u32 = 0xffff_ffff;
pub(crate) const HBIN_MAGIC: &[u8; 4] = b"hbin";
pub(crate) const MAX_KEY_LOOKUP_DEPTH: usize = 64;
pub(crate) const USER_ASSIST_ENTRY_SIZE: usize = 72;
pub(crate) const SAM_ACCOUNT_DISABLED: u32 = 0x0001;
pub(crate) const SAM_ACCOUNT_LOCKED: u32 = 0x0010;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRegistryField {
    pub value: String,
    pub hive_path: String,
    pub key_path: String,
    pub value_name: String,
    pub parser: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxlogTimestampInfo {
    pub field_name: String,
    pub hive_timestamp: Option<DateTime<Utc>>,
    pub txlog_timestamp: Option<DateTime<Utc>>,
    pub txlog_used: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SystemHiveInfo {
    pub computer_name: Option<ParsedRegistryField>,
    pub timezone: Option<ParsedRegistryField>,
    pub txlog_applied: bool,
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SoftwareHiveInfo {
    pub product_name: Option<ParsedRegistryField>,
    pub current_build: Option<ParsedRegistryField>,
    pub current_version: Option<ParsedRegistryField>,
    pub display_version: Option<ParsedRegistryField>,
    pub install_date: Option<ParsedRegistryField>,
    pub registered_owner: Option<ParsedRegistryField>,
    pub registered_organization: Option<ParsedRegistryField>,
    pub product_id: Option<ParsedRegistryField>,
    pub txlog_applied: bool,
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRunKey {
    pub key_path: String,
    pub value_name: String,
    pub command: String,
    pub timestamp: Option<String>,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WinlogonConfig {
    pub shell: Option<String>,
    pub userinit: Option<String>,
    pub notify: Option<String>,
    pub auto_admin_logon: Option<String>,
    pub default_domain_name: Option<String>,
    pub default_user_name: Option<String>,
    pub key_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LsaPackages {
    pub control_set: String,
    pub authentication_packages: Vec<String>,
    pub notification_packages: Vec<String>,
    pub security_packages: Vec<String>,
}
