use chrono::{DateTime, Utc};

// ── Constants ────────────────────────────────────────────────────────────────

pub(crate) const BASE_BLOCK_SIZE: usize = 0x1000;
pub(crate) const NK_SIGNATURE: &[u8; 2] = b"nk";
pub(crate) const VK_SIGNATURE: &[u8; 2] = b"vk";
pub(crate) const REG_SZ: u32 = 1;
pub(crate) const REG_EXPAND_SZ: u32 = 2;
pub(crate) const REG_DWORD: u32 = 4;
pub(crate) const REG_MULTI_SZ: u32 = 7;
pub(crate) const REG_QWORD: u32 = 11;
pub(crate) const INVALID_OFFSET: u32 = 0xFFFF_FFFF;
pub(crate) const HBIN_MAGIC: &[u8; 4] = b"hbin";
pub(crate) const MAX_KEY_LOOKUP_DEPTH: usize = 64;
pub(crate) const USER_ASSIST_ENTRY_SIZE: usize = 72;

/// User account control flags in the SAM V record.
pub(crate) const SAM_ACCOUNT_DISABLED: u32 = 0x0001;
pub(crate) const SAM_ACCOUNT_LOCKED: u32 = 0x0010;

// ── Public structs ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRegistryField {
    pub value: String,
    pub hive_path: String,
    pub key_path: String,
    pub value_name: String,
    pub parser: String,
}

/// Records which txlog entry (if any) was used to override a field value,
/// and the timestamps from both the hive and the transaction log.
#[derive(Debug, Clone)]
pub struct TxlogTimestampInfo {
    /// Human-readable name of the field (e.g. "ComputerName").
    pub field_name: String,
    /// Last-write timestamp from the hive record (currently always `None` because
    /// the standard extractor does not read key timestamps).
    pub hive_timestamp: Option<DateTime<Utc>>,
    /// Timestamp from the matching transaction-log entry, when one was applied.
    pub txlog_timestamp: Option<DateTime<Utc>>,
    /// `true` when the field's value was updated from a txlog entry.
    pub txlog_used: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SystemHiveInfo {
    pub computer_name: Option<ParsedRegistryField>,
    pub timezone: Option<ParsedRegistryField>,
    /// Whether any field was updated from the transaction log.
    pub txlog_applied: bool,
    /// Per-field record of txlog override decisions and timestamps.
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
    /// Whether any field was updated from the transaction log.
    pub txlog_applied: bool,
    /// Per-field record of txlog override decisions and timestamps.
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}

/// A single auto-start entry found under Run / RunOnce keys in NTUSER.DAT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRunKey {
    pub key_path: String,
    pub value_name: String,
    pub command: String,
    pub timestamp: Option<String>,
}

/// A single entry from the Explorer RecentDocs MRU list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentDoc {
    pub file_name: String,
    pub extension: String,
    pub last_accessed: Option<String>,
    pub lnk_target: Option<String>,
}

/// A single UserAssist entry (program execution tracking).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAssistEntry {
    pub executable_path: String,
    pub run_count: u32,
    pub last_run: Option<String>,
    pub focus_time_ms: u64,
    pub session_id: u32,
}

/// A mount-point entry from Explorer MountPoints2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPoint {
    pub drive_letter: Option<String>,
    pub volume_guid: Option<String>,
    pub last_mounted: Option<String>,
}

/// Aggregated information extracted from an NTUSER.DAT hive.
#[derive(Debug, Clone, Default)]
pub struct NtuserInfo {
    pub run_keys: Vec<RegistryRunKey>,
    pub recent_docs: Vec<RecentDoc>,
    pub ua_entries: Vec<UserAssistEntry>,
    pub typed_urls: Vec<String>,
    pub word_wheel_query: Vec<String>,
    pub mount_points: Vec<MountPoint>,
    /// Whether any field was updated from the transaction log.
    pub txlog_applied: bool,
    /// Per-field record of txlog override decisions and timestamps.
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}

// ── SAM hive structs ─────────────────────────────────────────────────────────

/// A local user account parsed from a SAM registry hive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamUser {
    pub username: String,
    pub rid: u32,
    pub full_name: String,
    pub comment: String,
    pub home_dir: String,
    pub profile_path: String,
    pub last_login: Option<DateTime<Utc>>,
    pub password_last_set: Option<DateTime<Utc>>,
    pub account_disabled: bool,
    pub account_locked: bool,
    pub admin_count: u32,
    pub group_memberships: Vec<String>,
}

/// A local group parsed from a SAM registry hive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamGroup {
    pub name: String,
    pub rid: u32,
    pub members: Vec<String>,
}

/// Aggregated information extracted from a SAM hive.
#[derive(Debug, Clone, Default)]
pub struct SamInfo {
    pub users: Vec<SamUser>,
    pub groups: Vec<SamGroup>,
    /// Domain-level password policy from `SAM\Domains\Account\F`.
    pub password_policy: Option<crate::registry::sam_structs::SamPasswordPolicy>,
    pub warnings: Vec<String>,
}

// ── Internal types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegistryValue {
    String(String),
    Dword(u32),
    Qword(u64),
    MultiString(Vec<String>),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone)]
pub(crate) struct NkRecord {
    pub(crate) name: String,
    pub(crate) num_subkeys: u32,
    pub(crate) subkeys_list_offset: u32,
    pub(crate) num_values: u32,
    pub(crate) values_list_offset: u32,
}
