use serde::{Deserialize, Serialize};

use crate::dto::analysis_base::AnalysisParseStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryExtractionSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub total: u64,
    pub values: Vec<RegistryValueDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryValueDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub hive_path: String,
    pub key_path: String,
    pub value_name: String,
    pub value_type: String,
    pub data: String,
    pub parser: String,
    pub created_at: String,
}

// SAM User Account (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamUserAccountDto {
    pub username: String,
    pub rid: u32,
    pub rid_hex: String,
    pub sid: String,
    pub groups: Vec<String>,
    pub login_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_created: Option<String>,
    pub account_status: String, // "enabled" | "disabled" | "locked"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash_type: Option<String>, // "NTLM" | "LM" | "Both"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hint: Option<String>,
    pub data_source_id: String,
    pub hive_path: String,
    pub key_path: String,
    pub parser: String,
}

// Registry Hive Overview
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHiveOverviewDto {
    pub hive_name: String,
    pub status: AnalysisParseStatusDto,
    pub key_value_count: u64,
    pub extracted_at: String,
    pub data_source_id: String,
    pub source_path: String,
    pub txlog_merged: bool,
    pub deleted_keys_found: u32,
}

// UserAssist Entry (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAssistEntryDto {
    pub program_path: String,
    pub exec_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exec_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_suspicious: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspicious_reason: Option<String>,
}

// Network Profile from SOFTWARE\NetworkList (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProfileDto {
    pub profile_guid: String,
    pub profile_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_last_connected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_type: Option<u32>,
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_network: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_gateway_mac_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_suffix: Option<String>,
    pub source_key_path: String,
}

/// A network adapter and its TCP/IP configuration recovered from the SYSTEM hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryNetworkAdapterDto {
    pub guid: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permanent_mac_address: Option<String>,
    pub ip_addresses: Vec<String>,
    pub subnet_masks: Vec<String>,
    pub gateways: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp_server: Option<String>,
    pub dns_servers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnp_instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
}

// Installed Software (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSoftwareDto {
    pub display_name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_suspicious: Option<bool>,
}

// USB Device History (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbDeviceHistoryDto {
    pub device_name: String,
    pub serial_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_connect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive_letter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_suspicious: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspicious_reason: Option<String>,
}

// Mounted Device (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountedDeviceDto {
    pub device_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive_letter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_signature_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
}

/// A single service or driver extracted from `SYSTEM\<ControlSet>\Services`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemServiceDto {
    pub service_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_dll: Option<String>,
    pub service_type: String,
    pub start_type: String,
    pub delayed_auto_start: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_control: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_name: Option<String>,
    pub depend_on_service: Vec<String>,
    pub depend_on_group: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_command: Option<String>,
    pub required_privileges: Vec<String>,
    pub key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_last_write: Option<String>,
}

/// A shutdown time entry parsed from the SYSTEM hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownTimeDto {
    pub key_path: String,
    pub shutdown_time: String,
}

/// A single AppCompatCache (ShimCache) entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShimCacheEntryDto {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    pub source_key_path: String,
}

/// Winlogon configuration fields from the SOFTWARE hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WinlogonConfigDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userinit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_admin_logon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_user_name: Option<String>,
    pub key_path: String,
}

/// A single AppCompatFlags\Layers entry from SOFTWARE or NTUSER.DAT.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCompatLayerDto {
    pub executable_path: String,
    pub layer_string: String,
    pub source_hive_path: String,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// LSA packages loaded for a control set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LsaPackageDto {
    pub control_set: String,
    pub authentication_packages: Vec<String>,
    pub notification_packages: Vec<String>,
    pub security_packages: Vec<String>,
}

/// Non-sensitive local security policy metadata from the SECURITY hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPolicyDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_domain_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_sid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_policy_hex: Option<String>,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// A single LSA secret entry from the SECURITY hive (controlled disclosure).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LsaSecretDto {
    pub secret_name: String,
    pub version: String,
    pub encrypted_blob_hex: String,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// A single cached domain credential entry from `SECURITY\Cache` (controlled disclosure).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedCredentialDto {
    pub entry_name: String,
    pub encrypted_blob_hex: String,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// A single OpenSavePidlMRU entry from NTUSER.DAT.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSaveMruEntryDto {
    pub extension: String,
    pub value_name: String,
    pub file_name: String,
    pub raw_pidl_hex: String,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// A single LastVisitedPidlMRU entry from NTUSER.DAT.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastVisitedMruEntryDto {
    pub value_name: String,
    pub path: String,
    pub raw_pidl_hex: String,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// A single RunMRU entry from NTUSER.DAT (Win+R run history).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMruEntryDto {
    pub value_name: String,
    pub command: String,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// A single Shellbag entry from UsrClass.dat.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellbagEntryDto {
    pub path: String,
    pub raw_pidl_hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_slot: Option<u32>,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// A single MuiCache entry from UsrClass.dat.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MuiCacheEntryDto {
    pub program_path: String,
    pub friendly_name: String,
    pub source_key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write: Option<String>,
}

/// A single installed application entry parsed from `Amcache.hve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmcacheApplicationDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version_at_install_time: Option<String>,
    pub registry_key_path: String,
}

/// A single application-file execution entry parsed from `Amcache.hve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmcacheApplicationFileDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_case_long_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_path_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_pe_file: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_date: Option<String>,
    pub registry_key_path: String,
}

// Registry Structured Summary (aggregates all structured views)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryStructuredSummaryDto {
    pub hive_overviews: Vec<RegistryHiveOverviewDto>,
    pub sam_users: Vec<SamUserAccountDto>,
    pub user_assist_entries: Vec<UserAssistEntryDto>,
    pub network_adapters: Vec<RegistryNetworkAdapterDto>,
    pub network_profiles: Vec<NetworkProfileDto>,
    pub installed_software: Vec<InstalledSoftwareDto>,
    pub usb_devices: Vec<UsbDeviceHistoryDto>,
    pub mounted_devices: Vec<MountedDeviceDto>,
    pub system_services: Vec<SystemServiceDto>,
    pub shutdown_times: Vec<ShutdownTimeDto>,
    pub shimcache_entries: Vec<ShimCacheEntryDto>,
    pub run_keys: Vec<crate::dto::registry::RegistryRunKeyDto>,
    pub open_save_mru: Vec<OpenSaveMruEntryDto>,
    pub last_visited_mru: Vec<LastVisitedMruEntryDto>,
    pub run_mru: Vec<RunMruEntryDto>,
    pub shellbag_entries: Vec<ShellbagEntryDto>,
    pub muicache_entries: Vec<MuiCacheEntryDto>,
    pub amcache_applications: Vec<AmcacheApplicationDto>,
    pub amcache_application_files: Vec<AmcacheApplicationFileDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winlogon_config: Option<WinlogonConfigDto>,
    pub lsa_packages: Vec<LsaPackageDto>,
    pub appcompat_layers: Vec<AppCompatLayerDto>,
    pub security_policies: Vec<SecurityPolicyDto>,
    pub lsa_secrets: Vec<LsaSecretDto>,
    pub cached_credentials: Vec<CachedCredentialDto>,
    pub status: AnalysisParseStatusDto,
    pub generated_at: String,
    pub warnings: Vec<String>,
}
