use super::{RegistryRunKey, TxlogTimestampInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentDoc {
    pub file_name: String,
    pub extension: String,
    pub last_accessed: Option<String>,
    pub lnk_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAssistEntry {
    pub executable_path: String,
    pub run_count: u32,
    pub last_run: Option<String>,
    pub focus_time_ms: u64,
    pub session_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpenSaveMruEntry {
    pub extension: String,
    pub value_name: String,
    pub file_name: String,
    pub raw_pidl_hex: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LastVisitedMruEntry {
    pub value_name: String,
    pub path: String,
    pub raw_pidl_hex: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunMruEntry {
    pub value_name: String,
    pub command: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShellbagEntry {
    pub path: String,
    pub raw_pidl_hex: String,
    pub node_slot: Option<u32>,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MuiCacheEntry {
    pub program_path: String,
    pub friendly_name: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPoint {
    pub drive_letter: Option<String>,
    pub volume_guid: Option<String>,
    pub last_mounted: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NtuserInfo {
    pub run_keys: Vec<RegistryRunKey>,
    pub recent_docs: Vec<RecentDoc>,
    pub ua_entries: Vec<UserAssistEntry>,
    pub typed_urls: Vec<String>,
    pub word_wheel_query: Vec<String>,
    pub mount_points: Vec<MountPoint>,
    pub open_save_mru: Vec<OpenSaveMruEntry>,
    pub last_visited_mru: Vec<LastVisitedMruEntry>,
    pub run_mru: Vec<RunMruEntry>,
    pub default_browser: Option<String>,
    pub txlog_applied: bool,
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}
