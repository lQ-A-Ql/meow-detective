//! Investigator-selectable guest resources and integration options for the
//! generated VMX, plus the conditional isolation rules they unlock.

use std::collections::BTreeMap;

use crate::vmx::invalid_vmx;
use crate::EmulationError;

/// Network attachment mode for the guest's single virtual NIC.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VmNetworkMode {
    /// No network adapter: the strongest isolation and the default.
    #[default]
    Off,
    /// Host-only network: guest and host can talk, nothing leaves the host.
    HostOnly,
    /// NAT: the guest reaches external networks through the host's address.
    Nat,
    /// Bridged: the guest appears as a peer on the host's physical network.
    Bridged,
}

impl VmNetworkMode {
    pub(crate) fn connection_type(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::HostOnly => Some("hostonly"),
            Self::Nat => Some("nat"),
            Self::Bridged => Some("bridged"),
        }
    }
}

/// Investigator-selectable guest resources and integrations. Resource values
/// default to 2 vCPUs and 4096 MiB; integrations default to off, and enabling
/// one loosens exactly one isolation control and is recorded in the session
/// provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmOptions {
    /// Network attachment mode for the guest NIC.
    pub network_mode: VmNetworkMode,
    /// Allow copy/paste and drag-and-drop between host and guest.
    pub clipboard: bool,
    /// Let VMware Tools synchronize the guest clock with the host.
    pub time_sync: bool,
    /// Virtual CPU count (1..=64).
    pub processor_count: u8,
    /// Guest memory in MiB (512..=262144).
    pub memory_mib: u32,
}

pub const MIN_PROCESSOR_COUNT: u8 = 1;
pub const MAX_PROCESSOR_COUNT: u8 = 64;
pub const MIN_MEMORY_MIB: u32 = 512;
pub const MAX_MEMORY_MIB: u32 = 262_144;

impl Default for VmOptions {
    fn default() -> Self {
        Self {
            network_mode: VmNetworkMode::Off,
            clipboard: false,
            time_sync: false,
            processor_count: 2,
            memory_mib: 4096,
        }
    }
}

impl VmOptions {
    pub fn validate(&self) -> Result<(), EmulationError> {
        if !(MIN_PROCESSOR_COUNT..=MAX_PROCESSOR_COUNT).contains(&self.processor_count) {
            return Err(invalid_vmx(format!(
                "processor count must be within {MIN_PROCESSOR_COUNT}..={MAX_PROCESSOR_COUNT}"
            )));
        }
        if !(MIN_MEMORY_MIB..=MAX_MEMORY_MIB).contains(&self.memory_mib) {
            return Err(invalid_vmx(format!(
                "memory size must be within {MIN_MEMORY_MIB}..={MAX_MEMORY_MIB} MiB"
            )));
        }
        Ok(())
    }
}

/// Guest OS identifiers the renderer will emit. Distro-specific ids come
/// from the VMware guest OS table; `other5xlinux-64` is the generic 64-bit
/// Linux 5.x fallback.
pub const GUEST_OS_WHITELIST: &[&str] = &[
    "windows9-64",
    "ubuntu-64",
    "debian12-64",
    "centos-64",
    "rhel8-64",
    "rhel9-64",
    "oraclelinux-64",
    "other5xlinux-64",
];

pub(crate) fn conditional_security_settings(
    options: VmOptions,
) -> Vec<(&'static str, &'static str)> {
    let mut settings = Vec::new();
    match options.network_mode.connection_type() {
        Some(connection_type) => {
            settings.push(("ethernet0.present", "TRUE"));
            settings.push(("ethernet0.connectionType", connection_type));
        }
        None => {
            settings.push(("ethernet0.present", "FALSE"));
        }
    }
    if !options.clipboard {
        settings.push(("isolation.tools.copy.disable", "TRUE"));
        settings.push(("isolation.tools.dnd.disable", "TRUE"));
        settings.push(("isolation.tools.paste.disable", "TRUE"));
    }
    if options.time_sync {
        settings.push(("tools.syncTime", "TRUE"));
    } else {
        settings.push(("time.synchronize.continue", "FALSE"));
        settings.push(("time.synchronize.restore", "FALSE"));
        settings.push(("time.synchronize.resume.disk", "FALSE"));
        settings.push(("time.synchronize.shrink", "FALSE"));
        settings.push(("tools.syncTime", "FALSE"));
    }
    settings
}

pub(crate) fn validate_isolation_exceptions(
    settings: &BTreeMap<String, String>,
    options: VmOptions,
) -> Result<(), EmulationError> {
    let ethernet_true = settings
        .iter()
        .any(|(key, value)| key.starts_with("ethernet") && value.eq_ignore_ascii_case("TRUE"));
    if options.network_mode == VmNetworkMode::Off && ethernet_true {
        return Err(invalid_vmx("network adapters must remain disabled"));
    }
    if options.network_mode != VmNetworkMode::Off
        && settings
            .keys()
            .any(|key| key.starts_with("ethernet") && !key.starts_with("ethernet0."))
    {
        return Err(invalid_vmx(
            "only the single configured adapter may be enabled",
        ));
    }
    if let Some(connection_type) = options.network_mode.connection_type() {
        if settings.get("ethernet0.connectionType").map(String::as_str) != Some(connection_type) {
            return Err(invalid_vmx(
                "ethernet0 connection type contradicts the network mode option",
            ));
        }
    }
    if options.clipboard
        && [
            "isolation.tools.copy.disable",
            "isolation.tools.dnd.disable",
            "isolation.tools.paste.disable",
        ]
        .into_iter()
        .any(|key| settings.get(key).map(String::as_str) == Some("TRUE"))
    {
        return Err(invalid_vmx(
            "clipboard isolation contradicts the clipboard option",
        ));
    }
    if options.time_sync
        && settings
            .iter()
            .any(|(key, value)| key.starts_with("time.synchronize") && value == "FALSE")
    {
        return Err(invalid_vmx(
            "time synchronization contradicts the time sync option",
        ));
    }
    Ok(())
}
