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

/// Conservative host-observed guest boot progress. This is intentionally
/// separate from the VMware power state: a powered-on VM is not necessarily
/// ready to accept an investigator login.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationGuestPhaseDto {
    #[default]
    Unknown,
    Booting,
    FilesystemMounted,
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
    /// `btrfs-root`, `xfs-log-dirty`, `xfs-log-unverified`,
    /// `ext4-journal-dirty`, `ext4-journal-unverified`); empty when none apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boot_risk_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationPreflightDto {
    pub data_source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_volume: Option<super::DirectNtfsVolumeDto>,
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
    pub guest_phase: EmulationGuestPhaseDto,
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

/// Outcome of a Linux account bypass attempt for the active emulation session.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationLinuxBypassStatusDto {
    /// The password was cleared (or was already configured) in the overlay.
    #[default]
    Applied,
    /// The bypass could not be applied on this layout; see `reason`/`alternatives`.
    Unavailable,
}

/// Which bypass channel served (or would serve) the edit.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationLinuxBypassMethodDto {
    /// Direct in-place edit on the mounted COW volume (XFS/ext4).
    #[default]
    HostInPlace,
    /// External-rescue path: a self-authored payload CD is staged for a
    /// user-supplied rescue ISO. The enum name is retained for wire
    /// compatibility; no Linux appliance image is bundled or downloaded.
    LinuxAppliance,
    /// No bypass channel available.
    None,
}

/// Why the host in-place bypass was refused; surfaced to the UI without raw
/// filesystem error text.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationLinuxBypassBlockerDto {
    /// Target inode shares its extents (XFS reflink); in-place edit would
    /// corrupt the sibling file.
    SharedExtents,
    /// The XFS internal log is dirty and cannot be cleared for this layout.
    DirtyLog,
    /// The volume is encrypted and the host cannot unlock it offline.
    EncryptedVolume,
    /// The filesystem layout cannot be edited in place (local forks, BMBT,
    /// NREXT64, sparse/unwritten extents, ...).
    UnsupportedLayout,
    /// No usable self-supplied rescue ISO is configured for the external
    /// rescue path.
    ApplianceUnavailable,
    /// The requested account was not found in /etc/shadow.
    AccountNotFound,
    /// The shadow hash scheme is not recognized by the offline editor.
    HashSchemeUnknown,
}

/// Suggested fallback when the preferred method is unavailable.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationLinuxBypassAlternativeDto {
    /// Boot the guest without an account bypass and log in normally.
    LaunchWithoutBypass,
    /// Attach an external rescue ISO for manual shadow editing.
    ExternalRescueIso,
    /// Retry the external-rescue path after attaching or reselecting a
    /// user-supplied rescue ISO. The enum name is retained for wire
    /// compatibility.
    RetryAppliance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationLinuxBypassResultDto {
    pub session_id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub username: String,
    pub password_set: bool,
    pub already_configured: bool,
    pub status: EmulationLinuxBypassStatusDto,
    pub method: EmulationLinuxBypassMethodDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<EmulationLinuxBypassBlockerDto>,
    #[serde(default)]
    pub alternatives: Vec<EmulationLinuxBypassAlternativeDto>,
    /// Whether the external-rescue payload CD was staged for this session.
    /// The frontend renders the corresponding instruction; the backend never
    /// returns user-facing prose.
    #[serde(default)]
    pub payload_staged: bool,
}

/// Read-only pre-flight inspection request; no overlay writes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationLinuxBypassInspectRequestDto {
    pub session_id: String,
    pub partition_index: u32,
}

/// Read-only assessment of the bypass options for one partition, gathered
/// before any edit is attempted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationLinuxBypassInspectionDto {
    pub session_id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub filesystem: String,
    /// Whether the volume advertises reflink (XFS `sb_features_ro_compat`).
    pub volume_reflink_enabled: bool,
    /// Whether the target inode itself carries the reflink flag; `None` when
    /// the inode could not be inspected.
    pub target_inode_reflink: Option<bool>,
    pub log_clean: bool,
    pub recommended_method: EmulationLinuxBypassMethodDto,
    pub blockers: Vec<EmulationLinuxBypassBlockerDto>,
    pub alternatives: Vec<EmulationLinuxBypassAlternativeDto>,
    pub appliance_available: bool,
}

/// Which loader the EFI fallback path was populated from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationEfiFallbackStrategyDto {
    /// `\EFI\<vendor>\shimx64.efi` plus its GRUB/MokManager companions.
    Shim,
    /// A vendor `grubx64.efi` copied directly as the fallback loader.
    Grub,
    /// `systemd-bootx64.efi` copied as the fallback loader.
    SystemdBoot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationEfiFallbackResultDto {
    pub session_id: String,
    pub data_source_id: String,
    /// GPT index of the EFI system partition that received the fallback.
    pub esp_partition_index: u32,
    pub strategy: Option<EmulationEfiFallbackStrategyDto>,
    pub files_written: Vec<String>,
    /// `\EFI\BOOT\BOOTX64.EFI` already existed; nothing was changed.
    pub already_present: bool,
}

/// The assessed state of one XFS volume's log before any repair ran.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationFsVolumeStateDto {
    /// Unmount record at the log head; nothing to replay.
    Clean,
    /// Pending transactions; the log-clear repair was applied.
    Dirty,
    /// External log device or unreadable layout; skipped.
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationFsRepairItemDto {
    pub partition_index: u32,
    /// State observed before any repair writes were applied.
    pub initial_state: EmulationFsVolumeStateDto,
    /// State verified from the session COW after the operation completed.
    pub state: EmulationFsVolumeStateDto,
    pub repaired: bool,
    pub log_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationFsRepairResultDto {
    pub session_id: String,
    pub data_source_id: String,
    pub items: Vec<EmulationFsRepairItemDto>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/emulation.rs"]
mod tests;
