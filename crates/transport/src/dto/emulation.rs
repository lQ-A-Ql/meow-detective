use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationNetworkModeDto {
    #[default]
    Off,
    HostOnly,
    Nat,
    Bridged,
}

fn default_processor_count() -> u8 {
    2
}

fn default_memory_mib() -> u32 {
    4096
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationOptionsDto {
    #[serde(default)]
    pub network_mode: EmulationNetworkModeDto,
    #[serde(default)]
    pub clipboard: bool,
    #[serde(default)]
    pub time_sync: bool,
    #[serde(default = "default_processor_count")]
    pub processor_count: u8,
    #[serde(default = "default_memory_mib")]
    pub memory_mib: u32,
}

impl Default for EmulationOptionsDto {
    fn default() -> Self {
        Self {
            network_mode: EmulationNetworkModeDto::Off,
            clipboard: false,
            time_sync: false,
            processor_count: default_processor_count(),
            memory_mib: default_memory_mib(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareEmulationRequestDto {
    pub data_source_id: String,
    #[serde(default)]
    pub recovery_iso_path: Option<String>,
    #[serde(default)]
    pub allow_direct_boot: bool,
    #[serde(default)]
    pub options: EmulationOptionsDto,
}

impl PrepareEmulationRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.data_source_id.trim().is_empty() {
            return Err("data source id is required");
        }
        if self
            .recovery_iso_path
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            return Err("recovery ISO path cannot be empty");
        }
        match (self.recovery_iso_path.is_some(), self.allow_direct_boot) {
            (false, false) => {
                return Err("direct system boot requires explicit confirmation");
            }
            (true, true) => {
                return Err("direct boot confirmation cannot be combined with recovery media");
            }
            _ => {}
        }
        if !(1..=64).contains(&self.options.processor_count) {
            return Err("processor count must be within 1..=64");
        }
        if !(512..=262_144).contains(&self.options.memory_mib) {
            return Err("memory size must be within 512..=262144 MiB");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationStateDto {
    DescriptorReady,
    Running,
    Quiescing,
    Released,
    FailedCleanupPending,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationControlModeDto {
    InteractiveOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationBootRouteDto {
    RecoveryMedia,
    DirectSystem,
}

/// Platform of a detected installation. Defaults to `windows` so pre-change
/// payloads keep parsing.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationInstallPlatformDto {
    #[default]
    Windows,
    Linux,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationInstallDto {
    pub partition_index: u32,
    #[serde(default)]
    pub platform: EmulationInstallPlatformDto,
    pub osdata_present: bool,
    pub sam_present: bool,
    pub utilman_bypass_available: bool,
    /// Whether the OSDATA directory is empty; `None` when OSDATA is absent,
    /// not a directory, or the filesystem could not be listed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub osdata_empty: Option<bool>,
    /// Linux: `PRETTY_NAME` from `/etc/os-release`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_release_pretty_name: Option<String>,
    /// Linux: a bootable kernel image exists under `/boot`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_present: Option<bool>,
    /// Linux: `/etc/fstab` exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fstab_present: Option<bool>,
    /// Linux: structured boot-risk annotations (e.g. `no-kernel`, `no-fstab`,
    /// `btrfs-root`); empty when none apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boot_risk_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationPreflightDto {
    pub data_source_id: String,
    pub installs: Vec<EmulationInstallDto>,
    pub recommended_boot_route: EmulationBootRouteDto,
    /// Set by the command layer: whether the WinPE maintenance tool binary is
    /// resolvable on this machine (the app-services layer does not know about
    /// tool packaging).
    #[serde(default)]
    pub maintenance_tool_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationSessionStatusDto {
    pub session_id: String,
    pub data_source_id: String,
    pub state: EmulationStateDto,
    pub logical_length: u64,
    pub control_mode: EmulationControlModeDto,
    #[serde(default)]
    pub maintenance_media: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationBypassAccountDto {
    pub rid: u32,
    pub username: String,
    pub disabled: bool,
    pub locked_out: bool,
    pub has_password: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationBypassActionDto {
    ClearPassword,
    EnableAndClearPassword,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationBypassApplyRequestDto {
    pub session_id: String,
    pub partition_index: u32,
    pub rid: u32,
    pub action: EmulationBypassActionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationBypassResultDto {
    pub session_id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub rid: u32,
    pub username: String,
    pub password_cleared: bool,
    pub account_enabled: bool,
    pub already_passwordless: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationOsdataCleanupRequestDto {
    pub session_id: String,
    pub partition_index: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationOsdataCleanupStateDto {
    /// The entry was excised from the parent index and the record retired.
    Removed,
    /// No `OSDATA` entry exists under `Windows/System32/config`.
    Absent,
    /// `OSDATA` is a non-empty directory; host-side removal is refused.
    RefusedNonEmpty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationOsdataCleanupDto {
    pub session_id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub state: EmulationOsdataCleanupStateDto,
    pub edits_applied: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationLinuxAccountDto {
    pub username: String,
    pub has_password: bool,
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationLinuxBypassRequestDto {
    pub session_id: String,
    pub partition_index: u32,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationLinuxBypassResultDto {
    pub session_id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub username: String,
    pub password_cleared: bool,
    pub already_passwordless: bool,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/emulation.rs"]
mod tests;
