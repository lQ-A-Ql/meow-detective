//! Linux rescue payload staging for the manual shadow bypass path.
//!
//! When the host cannot rewrite `/etc/shadow` in place (XFS reflink shared
//! extents, dirty log, or an unsupported layout), the product never fabricates
//! an automatic appliance boot. It stages a self-authored payload CD
//! (`SHADOW.NEW` + `BYPASS.JSON` + `BYPASS.SH`) on the session's second optical
//! drive; the investigator boots from their own rescue ISO and runs
//! `BYPASS.SH` manually. No operating-system image, GPL/LGPL component, or
//! third-party tool is bundled, downloaded, or linked.
//!
//! Staging only runs while the session is `DescriptorReady` and the caller
//! holds the per-session `op_lock`: the main VM and the rescue environment must
//! never open the same `disk.raw` at the same time.

use std::path::{Path, PathBuf};

use evidence_emulation::{build_iso, IsoFile, VmOptions, VmdkAdapter, VmwareFirmware, VmxConfig};
use transport::dto::EmulationLinuxBypassBlockerDto;

use super::recovery_media;
use super::workspace::SessionWorkspace;
use super::EmulationRegistryError;

/// Files carried on the payload CD. The ISO9660 Level 2 writer appends the
/// `;1` version suffix, so the guest sees these names with the suffix; the
/// manifest records both the plain and the on-disk names.
const SHADOW_FILE_NAME: &str = "SHADOW.NEW";
const MANIFEST_FILE_NAME: &str = "BYPASS.JSON";
const SCRIPT_FILE_NAME: &str = "BYPASS.SH";

/// Self-authored manual aid; no third-party code.
const BYPASS_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../resources/emulation/linux-rescue/BYPASS.SH"
));

/// Inputs for the payload CD; every value originates from the read-only plan.
pub(super) struct RescuePayloadInput<'a> {
    pub partition_index: u32,
    pub filesystem: &'a str,
    pub username: &'a str,
    pub blocker: EmulationLinuxBypassBlockerDto,
    pub edited: &'a str,
}

/// Minimal VMX rebuild inputs captured at prepare time so the payload CD can
/// be attached as `ide1:1` without re-probing the source volume.
#[derive(Clone)]
pub(super) struct VmxBlueprint {
    firmware: VmwareFirmware,
    guest_os: String,
    disk_adapter: VmdkAdapter,
    network_pci_slot: u16,
    options: VmOptions,
    recovery_iso: Option<String>,
}

impl VmxBlueprint {
    pub(super) fn new(
        firmware: VmwareFirmware,
        guest_os: String,
        disk_adapter: VmdkAdapter,
        network_pci_slot: u16,
        options: VmOptions,
        recovery_iso: Option<String>,
    ) -> Self {
        Self {
            firmware,
            guest_os,
            disk_adapter,
            network_pci_slot,
            options,
            recovery_iso,
        }
    }

    pub(super) fn recovery_iso(&self) -> Option<&str> {
        self.recovery_iso.as_deref()
    }
}

/// Resolves the session's investigator-supplied rescue ISO. Only a locally
/// configured, readable regular file qualifies; the product never downloads
/// or bundles an operating-system image, so a missing file simply means no
/// rescue channel.
pub(super) fn rescue_iso_for_session(recovery_iso: Option<&Path>) -> Option<PathBuf> {
    let path = recovery_iso?;
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    // A readability probe: an unreadable configured path is not a usable ISO.
    std::fs::File::open(path).ok()?;
    Some(path.to_path_buf())
}

fn blocker_label(blocker: EmulationLinuxBypassBlockerDto) -> String {
    serde_json::to_value(blocker)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Builds the payload CD bytes (`SHADOW.NEW`, `BYPASS.JSON`, `BYPASS.SH`)
/// without touching disk. Pure so unit tests can parse the image directly.
pub(super) fn build_rescue_payload_iso(
    input: &RescuePayloadInput<'_>,
) -> Result<Vec<u8>, EmulationRegistryError> {
    let edited = input.edited.as_bytes();
    let shadow_sha256 = infrastructure::hashing::sha256_bytes(edited);
    let blocker = blocker_label(input.blocker);
    let manifest = RescueManifest {
        schema_version: 1,
        partition_index: input.partition_index,
        filesystem: input.filesystem,
        username: input.username,
        blocker: &blocker,
        shadow_sha256,
        size_bytes: edited.len() as u64,
        generated_at: chrono::Utc::now().to_rfc3339(),
        shadow_file_name: SHADOW_FILE_NAME,
        manifest_file_name: MANIFEST_FILE_NAME,
        script_file_name: SCRIPT_FILE_NAME,
        shadow_on_disk_name: "SHADOW.NEW;1",
        manifest_on_disk_name: "BYPASS.JSON;1",
        script_on_disk_name: "BYPASS.SH;1",
        script_usage: "BYPASS.SH --disk <root partition>",
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    let files = [
        IsoFile {
            name: SHADOW_FILE_NAME,
            data: edited,
        },
        IsoFile {
            name: MANIFEST_FILE_NAME,
            data: &manifest_bytes,
        },
        IsoFile {
            name: SCRIPT_FILE_NAME,
            data: BYPASS_SCRIPT.as_bytes(),
        },
    ];
    build_iso(&files).map_err(EmulationRegistryError::Disk)
}

/// Stages the payload CD into the session workspace and attaches it as the
/// second optical drive (`ide1:1`) of the published VMX. Caller must hold the
/// session `op_lock` and keep the session in `DescriptorReady`; this function
/// never opens `disk.raw` and never invalidates the overlay.
pub(super) fn stage_rescue_payload(
    workspace: &SessionWorkspace,
    blueprint: &VmxBlueprint,
    input: &RescuePayloadInput<'_>,
) -> Result<PathBuf, EmulationRegistryError> {
    let bytes = build_rescue_payload_iso(input)?;
    let payload_path = workspace
        .write_payload_iso(&bytes)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    attach_payload_cd(workspace, blueprint, &payload_path)?;
    Ok(payload_path)
}

fn attach_payload_cd(
    workspace: &SessionWorkspace,
    blueprint: &VmxBlueprint,
    payload_path: &Path,
) -> Result<(), EmulationRegistryError> {
    let mut vmx = VmxConfig::new("disk.vmdk", blueprint.firmware)
        .map_err(EmulationRegistryError::Disk)?
        .with_guest_os(&blueprint.guest_os)
        .map_err(EmulationRegistryError::Disk)?
        .with_disk_adapter(blueprint.disk_adapter)
        .with_linux_network_interface_index(blueprint.network_pci_slot)
        .with_options(blueprint.options)
        .map_err(EmulationRegistryError::Disk)?;
    if let Some(recovery) = &blueprint.recovery_iso {
        vmx = vmx
            .with_recovery_iso(recovery)
            .map_err(EmulationRegistryError::Disk)?;
    }
    let payload_text = recovery_media::vmware_path(payload_path)
        .map_err(|error| EmulationRegistryError::RecoveryMedia(error.to_string()))?;
    vmx = vmx
        .with_maintenance_iso(&payload_text)
        .map_err(EmulationRegistryError::Disk)?;
    let rendered = vmx.render();
    VmxConfig::validate_rendered(&rendered, blueprint.options, true)
        .map_err(EmulationRegistryError::Disk)?;
    workspace
        .replace_vmx(&rendered)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RescueManifest<'a> {
    schema_version: u32,
    partition_index: u32,
    filesystem: &'a str,
    username: &'a str,
    blocker: &'a str,
    shadow_sha256: String,
    size_bytes: u64,
    generated_at: String,
    shadow_file_name: &'static str,
    manifest_file_name: &'static str,
    script_file_name: &'static str,
    shadow_on_disk_name: &'static str,
    manifest_on_disk_name: &'static str,
    script_on_disk_name: &'static str,
    script_usage: &'static str,
}

#[cfg(test)]
#[path = "../../tests/unit/emulation_registry/linux_rescue.rs"]
mod tests;
