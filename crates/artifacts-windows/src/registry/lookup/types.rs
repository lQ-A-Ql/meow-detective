use chrono::{DateTime, Utc};

// ── Constants ────────────────────────────────────────────────────────────────

pub(crate) const BASE_BLOCK_SIZE: usize = 0x1000;
pub(crate) const NK_SIGNATURE: &[u8; 2] = b"nk";
pub(crate) const VK_SIGNATURE: &[u8; 2] = b"vk";
pub(crate) const REG_SZ: u32 = 1;
pub(crate) const REG_EXPAND_SZ: u32 = 2;
#[cfg(test)]
pub(crate) const REG_BINARY: u32 = 3;
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// A single auto-start entry found under Run / RunOnce keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRunKey {
    pub key_path: String,
    pub value_name: String,
    pub command: String,
    pub timestamp: Option<String>,
    /// "machine" for HKLM Run/RunOnce, "user" for HKCU Run/RunOnce.
    pub scope: String,
}

/// Winlogon configuration fields from the SOFTWARE hive.
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

/// LSA authentication/notification/security packages from a SYSTEM control set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LsaPackages {
    pub control_set: String,
    pub authentication_packages: Vec<String>,
    pub notification_packages: Vec<String>,
    pub security_packages: Vec<String>,
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

/// A single OpenSavePidlMRU entry from NTUSER.DAT.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpenSaveMruEntry {
    pub extension: String,    // e.g. "*", "txt", "pdf"
    pub value_name: String,   // MRU value name (e.g. "0", "1")
    pub file_name: String,    // Best-effort decoded file name
    pub raw_pidl_hex: String, // Hex-encoded PIDL blob for manual analysis
    pub source_key_path: String,
    pub last_write: Option<String>,
}

/// A single LastVisitedPidlMRU entry from NTUSER.DAT.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LastVisitedMruEntry {
    pub value_name: String,
    pub path: String, // Best-effort decoded path
    pub raw_pidl_hex: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

/// A single RunMRU entry from NTUSER.DAT (Win+R run history).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunMruEntry {
    pub value_name: String,
    pub command: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

/// A single Shellbag entry from UsrClass.dat BagMRU.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShellbagEntry {
    pub path: String,
    pub raw_pidl_hex: String,
    pub node_slot: Option<u32>,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

/// A single MuiCache entry from UsrClass.dat.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MuiCacheEntry {
    pub program_path: String,
    pub friendly_name: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
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
    pub open_save_mru: Vec<OpenSaveMruEntry>,
    pub last_visited_mru: Vec<LastVisitedMruEntry>,
    pub run_mru: Vec<RunMruEntry>,
    /// Default browser ProgId, e.g. `ChromeHTML` or `MSEdgeHTM`.
    pub default_browser: Option<String>,
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
    /// Hex-encoded password hashes in `lm:nt` form when decryption succeeds.
    pub password_hash: Option<String>,
    /// Which hash types are present: `LM`, `NTLM`, `Both`, or `None`.
    pub password_hash_type: Option<String>,
}

/// A local group parsed from a SAM registry hive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamGroup {
    pub name: String,
    pub rid: u32,
    pub members: Vec<String>,
}

/// A single installed software entry from the SOFTWARE hive Uninstall keys.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstalledSoftwareInfo {
    pub display_name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub install_date: Option<String>,
    /// Estimated size in kilobytes, as stored in the registry.
    pub estimated_size_kb: Option<u64>,
    pub uninstall_string: Option<String>,
    pub source_key: String,
}

/// Network adapter configuration extracted from the SYSTEM hive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkAdapterInfo {
    pub guid: String,
    pub name: Option<String>,
    pub mac_address: Option<String>,
    pub ip_address: Option<String>,
    pub subnet_mask: Option<String>,
    pub gateway: Option<String>,
    pub dhcp_server: Option<String>,
    pub dhcp_enabled: Option<bool>,
    pub dns_servers: Vec<String>,
}

/// Network profile entry extracted from the SOFTWARE hive NetworkList keys.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkProfileEntry {
    pub profile_guid: String,
    pub profile_name: String,
    pub description: Option<String>,
    pub date_created: Option<String>,
    pub date_last_connected: Option<String>,
    pub name_type: Option<u32>,
    pub managed: bool,
    pub first_network: Option<String>,
    pub default_gateway_mac_hex: Option<String>,
    pub dns_suffix: Option<String>,
    pub source_key_path: String,
}

/// SCM start type for a Windows service or driver.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ServiceStartType {
    #[default]
    Boot,
    System,
    Automatic,
    AutomaticDelayed,
    Manual,
    Disabled,
    Unknown(u32),
}

impl ServiceStartType {
    pub fn from_raw(start: u32, delayed_auto_start: bool) -> Self {
        match start {
            0 => ServiceStartType::Boot,
            1 => ServiceStartType::System,
            2 if delayed_auto_start => ServiceStartType::AutomaticDelayed,
            2 => ServiceStartType::Automatic,
            3 => ServiceStartType::Manual,
            4 => ServiceStartType::Disabled,
            other => ServiceStartType::Unknown(other),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceStartType::Boot => "Boot",
            ServiceStartType::System => "System",
            ServiceStartType::Automatic => "Automatic",
            ServiceStartType::AutomaticDelayed => "Automatic (Delayed Start)",
            ServiceStartType::Manual => "Manual",
            ServiceStartType::Disabled => "Disabled",
            ServiceStartType::Unknown(_) => "Unknown",
        }
    }
}

/// SCM service type (kernel driver, user-mode service, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ServiceType {
    #[default]
    KernelDriver,
    FileSystemDriver,
    Win32OwnProcess,
    Win32OwnProcessInteractive,
    Win32ShareProcess,
    Win32ShareProcessInteractive,
    Unknown(u32),
}

impl ServiceType {
    pub fn from_raw(raw: u32) -> Self {
        let interactive = raw & 0x100 != 0;
        let base = raw & 0xFF;
        if base == 0x01 {
            ServiceType::KernelDriver
        } else if base == 0x02 {
            ServiceType::FileSystemDriver
        } else if base == 0x10 {
            if interactive {
                ServiceType::Win32OwnProcessInteractive
            } else {
                ServiceType::Win32OwnProcess
            }
        } else if base == 0x20 {
            if interactive {
                ServiceType::Win32ShareProcessInteractive
            } else {
                ServiceType::Win32ShareProcess
            }
        } else {
            ServiceType::Unknown(raw)
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceType::KernelDriver => "Kernel Driver",
            ServiceType::FileSystemDriver => "File System Driver",
            ServiceType::Win32OwnProcess => "Own Process",
            ServiceType::Win32OwnProcessInteractive => "Own Process (Interactive)",
            ServiceType::Win32ShareProcess => "Share Process",
            ServiceType::Win32ShareProcessInteractive => "Share Process (Interactive)",
            ServiceType::Unknown(_) => "Unknown",
        }
    }
}

/// A single service or driver entry from `SYSTEM\<ControlSet>\Services`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemServiceEntry {
    pub service_name: String,
    pub display_name: Option<String>,
    pub image_path: Option<String>,
    pub service_dll: Option<String>,
    pub service_type: ServiceType,
    pub start_type: ServiceStartType,
    pub delayed_auto_start: bool,
    pub error_control: Option<u32>,
    pub group: Option<String>,
    pub object_name: Option<String>,
    pub depend_on_service: Vec<String>,
    pub depend_on_group: Vec<String>,
    pub failure_command: Option<String>,
    pub required_privileges: Vec<String>,
    pub key_path: String,
    pub key_last_write: Option<String>,
}

/// Aggregated service/driver list extracted from a SYSTEM hive.
#[derive(Debug, Clone, Default)]
pub struct SystemServiceInfo {
    pub services: Vec<SystemServiceEntry>,
    pub warnings: Vec<String>,
}

/// A single USB device history entry parsed from `SYSTEM\Enum\USBSTOR`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsbDeviceHistoryEntry {
    pub device_name: String,
    pub serial_number: String,
    pub raw_serial_number: String,
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub revision: Option<String>,
    pub first_connect: Option<String>,
    pub last_connect: Option<String>,
}

/// A single mounted-device entry parsed from `SYSTEM\MountedDevices`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MountedDeviceEntry {
    pub device_name: String,
    pub drive_letter: Option<String>,
    pub volume_guid: Option<String>,
    pub disk_signature_hex: Option<String>,
    pub target_name: Option<String>,
}

/// A shutdown time entry parsed from `SYSTEM\<ControlSet>\Control\Windows\ShutdownTime`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShutdownTimeEntry {
    pub key_path: String,
    pub shutdown_time: String,
}

/// Non-sensitive local security policy metadata extracted from the SECURITY hive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityPolicyEntry {
    pub domain_name: Option<String>, // from Policy\PolPrDmS value data
    pub account_domain_name: Option<String>, // from Policy\PolAcDmS value data
    pub machine_sid: Option<String>, // from Policy\PolMachineAccountS binary SID
    pub audit_policy_hex: Option<String>, // raw hex of Policy\PolAdtEv binary
    pub source_key_path: String,
    pub last_write: Option<String>,
    /// Whether any field was updated from the transaction log.
    pub txlog_applied: bool,
    /// Per-field record of txlog override decisions and timestamps.
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
}

/// A single LSA secret entry enumerated from the SECURITY hive.
/// Only metadata and the encrypted blob are exposed; no decryption is performed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LsaSecretEntry {
    pub secret_name: String,
    pub version: String, // "current" or "backup"
    pub encrypted_blob_hex: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

/// A single cached domain credential entry enumerated from `SECURITY\Cache`.
/// Only the entry name, encrypted blob, source key path, and last-write
/// timestamp are exposed. Decryption is intentionally not performed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CachedCredentialEntry {
    pub entry_name: String, // e.g., NL$1
    pub encrypted_blob_hex: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

/// A single AppCompatCache (ShimCache) entry parsed from the SYSTEM hive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShimCacheEntry {
    pub path: String,
    pub last_modified: Option<String>,
    pub source_key_path: String,
}

/// A single AppCompatFlags\Layers entry parsed from SOFTWARE or NTUSER.DAT.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppCompatLayerEntry {
    /// Value name: the executable path or short name.
    pub executable_path: String,
    /// Value data: the layer/elevation flags string.
    pub layer_string: String,
    /// Normalized hive path (e.g. `Windows/System32/config/SOFTWARE`).
    pub source_hive_path: String,
    /// Registry key path where the value was found.
    pub source_key_path: String,
    /// Key last-write timestamp as an RFC 3339 string, when available.
    pub last_write: Option<String>,
}

/// A single installed application entry parsed from `Amcache.hve`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmcacheApplicationEntry {
    pub program_id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub install_date: Option<String>,
    pub source: Option<String>,
    pub os_version_at_install_time: Option<String>,
    pub registry_key_path: String,
}

/// A single application-file execution entry parsed from `Amcache.hve`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmcacheApplicationFileEntry {
    pub program_id: Option<String>,
    pub lower_case_long_path: Option<String>,
    pub long_path_hash: Option<String>,
    pub file_size: Option<u64>,
    pub product_name: Option<String>,
    pub company_name: Option<String>,
    pub file_version: Option<String>,
    pub is_pe_file: Option<bool>,
    pub link_date: Option<String>,
    pub registry_key_path: String,
}

/// Aggregated information extracted from a SAM hive.
#[derive(Debug, Clone, Default)]
pub struct SamInfo {
    pub users: Vec<SamUser>,
    pub groups: Vec<SamGroup>,
    /// Domain-level password policy from `SAM\Domains\Account\F`.
    pub password_policy: Option<crate::registry::sam_structs::SamPasswordPolicy>,
    /// Whether any field was updated from the transaction log.
    pub txlog_applied: bool,
    /// Per-field record of txlog override decisions and timestamps.
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
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
    pub(crate) last_write_time: Option<u64>,
    pub(crate) num_subkeys: u32,
    pub(crate) subkeys_list_offset: u32,
    pub(crate) num_values: u32,
    pub(crate) values_list_offset: u32,
}
