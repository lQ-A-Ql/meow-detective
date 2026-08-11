//! Machine material generation: VMDK descriptor, VMX configuration,
//! provenance, and the optional maintenance CD-ROM that carries the WinPE
//! helper tool plus the import-index-derived target manifest into the guest.

use std::path::{Path, PathBuf};

use domain::{CaseId, DataSourceId};
use evidence_block::EvidenceImageKind;
use evidence_emulation::{
    build_iso, CowDisk, EmulationError, IsoFile, ParentIdentity, VmOptions, VmdkAdapter,
    VmdkDescriptor, VmwareFirmware, VmxConfig,
};

use super::recovery_media::RecoveryMedia;
use super::workspace::{
    EmulationProvenance, ProvenanceIds, RecoveryMediaProvenance, SessionWorkspace,
};
use super::EmulationRegistryError;

/// The helper executable is delivered read-only inside the guest; anything
/// larger indicates a packaging mistake rather than a legitimate build.
const MAX_MAINTENANCE_TOOL_BYTES: u64 = 64 * 1024 * 1024;

const MAINTENANCE_README: &str = "Meow~Detective maintenance media\r\n\
\r\n\
1. Open a command prompt in WinPE.\r\n\
2. Locate this CD-ROM drive (it contains TARGETS.JSON and MEOWMTN.EXE).\r\n\
3. Run: MEOWMTN.EXE run\r\n\
   The tool reads TARGETS.JSON and cross-checks the expected OSDATA and\r\n\
   Utilman-bypass state against what the guest actually sees; any mismatch\r\n\
   aborts before changes are made. When the check passes it removes a\r\n\
   leftover OSDATA node, and it can apply or restore the Utilman logon\r\n\
   bypass on request (see MEOWMTN.EXE usage output).\r\n\
   Auto-detection supports exactly one offline Windows installation; pass\r\n\
   --drive <letter> (e.g. MEOWMTN.EXE run --drive D:) to pick one.\r\n\
\r\n\
All writes land on the copy-on-write overlay; the evidence image is never\r\n\
modified.\r\n";

/// Manual delivered on the Linux rescue CD (no in-guest tool exists for
/// Linux; the CD carries the TARGETS.JSON map and this guide instead).
pub(super) const LINUX_RESCUE_README: &str = "Meow~Detective Linux rescue media\r\n\
\r\n\
This CD accompanies a user-selected Linux live/rescue ISO. It contains\r\n\
TARGETS.JSON: the host-side preflight map of the Linux installations\r\n\
(distro, partitions, boot-risk notes) for the disk attached to this VM.\r\n\
\r\n\
Suggested workflow inside the live system:\r\n\
1. Identify the disk: lsblk -f  (the evidence disk appears read-only to\r\n\
   the host; guest writes land on the copy-on-write overlay only).\r\n\
2. Inspect mounts from TARGETS.JSON, then mount the root volume, e.g.\r\n\
      mount -o ro /dev/sda3 /mnt        # read-only inspection\r\n\
3. Account recovery without host tooling:\r\n\
   - simplest: reboot, press 'e' in GRUB, append init=/bin/bash to the\r\n\
     linux line, Ctrl-X; then 'mount -o remount,rw /' and 'passwd'.\r\n\
   - offline: edit /mnt/etc/shadow and clear the second (hash) field of\r\n\
     the target account, then remount read-only again.\r\n\
4. Boot repairs: chroot /mnt and rebuild grub.cfg or the initramfs when\r\n\
   TARGETS.JSON reports no-kernel/no-fstab style risks.\r\n\
\r\n\
All writes land on the copy-on-write overlay; the evidence image is never\r\n\
modified.\r\n";

/// Structured TARGETS.JSON content for the Linux rescue CD. The Windows PE
/// tool consumes its own flat preflight shape; this envelope is the
/// Linux-side generalization and has no in-guest consumer yet — it is
/// documentation for the investigator.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LinuxRescueTargets<'a> {
    schema_version: u32,
    platform: &'static str,
    data_source_id: &'a str,
    installs: &'a [transport::dto::EmulationInstallDto],
    recommended_actions: &'static [&'static str],
}

const LINUX_RESCUE_ACTIONS: &[&str] = &[
    "boot-live-iso",
    "inspect-targets-json",
    "grub-init-bash-bypass",
    "offline-shadow-edit",
    "chroot-repair",
];

pub(super) fn linux_rescue_targets_json(
    data_source_id: &str,
    installs: &[transport::dto::EmulationInstallDto],
) -> Result<Vec<u8>, EmulationRegistryError> {
    let targets = LinuxRescueTargets {
        schema_version: 1,
        platform: "linux",
        data_source_id,
        installs,
        recommended_actions: LINUX_RESCUE_ACTIONS,
    };
    serde_json::to_vec_pretty(&targets)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))
}

pub(super) struct MaintenancePayload {
    pub tool: Option<Vec<u8>>,
    pub targets_json: Vec<u8>,
    pub readme: &'static str,
}

pub(super) fn build_maintenance_payload(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<Option<MaintenancePayload>, EmulationRegistryError> {
    let Some(tool_path) = resolve_maintenance_tool() else {
        tracing::warn!("WinPE maintenance tool was not found; maintenance media is skipped");
        return Ok(None);
    };
    let metadata = std::fs::metadata(&tool_path)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    if metadata.len() > MAX_MAINTENANCE_TOOL_BYTES {
        return Err(EmulationRegistryError::Workspace(
            "maintenance tool exceeds the delivery size limit".to_string(),
        ));
    }
    let tool = std::fs::read(&tool_path)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    let preflight = app_services::mount_service::emulation_preflight(
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )
    .map_err(EmulationRegistryError::Source)?;
    let targets_json = serde_json::to_vec_pretty(&preflight)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    Ok(Some(MaintenancePayload {
        tool: Some(tool),
        targets_json,
        readme: MAINTENANCE_README,
    }))
}

/// Linux rescue media: no in-guest tool, just the TARGETS.JSON map and the
/// rescue README. Built whenever the user supplies a live/rescue ISO.
pub(super) fn build_linux_rescue_payload(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<Option<MaintenancePayload>, EmulationRegistryError> {
    let preflight = app_services::mount_service::emulation_preflight(
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )
    .map_err(EmulationRegistryError::Source)?;
    let targets_json = linux_rescue_targets_json(&data_source_id.0, &preflight.installs)?;
    Ok(Some(MaintenancePayload {
        tool: None,
        targets_json,
        readme: LINUX_RESCUE_README,
    }))
}

pub(crate) fn maintenance_tool_available() -> bool {
    resolve_maintenance_tool().is_some()
}

/// Resolution order: explicit environment override, a `tools/` directory next
/// to the running application, then the workspace build output used in
/// development.
fn resolve_maintenance_tool() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("MEOW_MAINTENANCE_TOOL") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return Some(path);
        }
    }
    let file_name = "meow-winpe-maintenance.exe";
    if let Ok(executable) = std::env::current_exe() {
        let bundled = executable.parent()?.join("tools").join(file_name);
        if bundled.is_file() {
            return Some(bundled);
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packaged = manifest_dir.join("resources").join("tools").join(file_name);
    if packaged.is_file() {
        return Some(packaged);
    }
    let dev = manifest_dir.join("../../target/release").join(file_name);
    dev.is_file().then_some(dev)
}

/// The guest identity the VMX/VMDK pair is rendered for.
pub(super) struct MachineSpec<'a> {
    pub firmware: VmwareFirmware,
    pub guest_os: &'a str,
    pub disk_adapter: VmdkAdapter,
}

pub(super) fn prepare_machine_materials(
    workspace: &SessionWorkspace,
    identity: &ParentIdentity,
    spec: MachineSpec<'_>,
    ids: ProvenanceIds<'_>,
    recovery_media: Option<&RecoveryMedia>,
    options: VmOptions,
    maintenance: Option<&MaintenancePayload>,
) -> Result<(), EmulationRegistryError> {
    // Windows and the user-selected PE boot from the inbox IDE path; Linux
    // guests get LsiLogic (the driver ships in the kernel and mainstream
    // initramfs images). The caller picks the adapter per platform.
    let disk_adapter = spec.disk_adapter;
    let descriptor = VmdkDescriptor::new(identity, "mount/disk.raw", disk_adapter)?;
    let rendered = descriptor.render();
    if VmdkDescriptor::parse(&rendered)? != descriptor {
        return Err(EmulationRegistryError::Workspace(
            "VMDK descriptor round-trip validation failed".to_string(),
        ));
    }
    let reported_length = workspace
        .extent_length()
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    if reported_length != identity.logical_length() {
        return Err(EmulationRegistryError::Workspace(
            "mounted extent length does not match the evidence disk".to_string(),
        ));
    }
    workspace
        .write_vmdk(&rendered)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    let mut vmx = VmxConfig::new("disk.vmdk", spec.firmware)?
        .with_guest_os(spec.guest_os)?
        .with_disk_adapter(disk_adapter)
        .with_options(options)?;
    if let Some(media) = recovery_media {
        vmx = vmx.with_recovery_iso(media.vmware_path())?;
    }
    if let Some(payload) = maintenance {
        let mut iso_files = vec![
            IsoFile {
                name: "TARGETS.JSON",
                data: &payload.targets_json,
            },
            IsoFile {
                name: "README.TXT",
                data: payload.readme.as_bytes(),
            },
        ];
        if let Some(tool) = &payload.tool {
            iso_files.insert(
                0,
                IsoFile {
                    name: "MEOWMTN.EXE",
                    data: tool,
                },
            );
        }
        let iso = build_iso(&iso_files)?;
        let iso_path = workspace
            .write_maintenance_iso(&iso)
            .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
        let iso_text = iso_path.to_str().ok_or_else(|| {
            EmulationRegistryError::Workspace(
                "maintenance ISO path is not valid Unicode".to_string(),
            )
        })?;
        vmx = vmx.with_maintenance_iso(iso_text)?;
    }
    let rendered_vmx = vmx.render();
    VmxConfig::validate_rendered(&rendered_vmx, options, maintenance.is_some())?;
    workspace
        .write_vmx(&rendered_vmx)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    workspace
        .write_provenance(&EmulationProvenance::new(
            ids,
            identity,
            spec.firmware,
            options,
            maintenance.is_some(),
            recovery_media.map(|media| RecoveryMediaProvenance {
                file_name: media.file_name(),
                length: media.length(),
                sha256: media.sha256(),
            }),
        ))
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))
}

pub(super) fn detect_firmware(disk: &CowDisk) -> Result<VmwareFirmware, EmulationError> {
    if disk.len() < 1024 {
        return Ok(VmwareFirmware::Bios);
    }
    let mut gpt_header = [0u8; 512];
    disk.read_exact_at(512, &mut gpt_header)?;
    if &gpt_header[..8] != b"EFI PART" {
        return Ok(VmwareFirmware::Bios);
    }
    // A GPT disk carrying GRUB's BIOS boot partition boots legacy: the core
    // image embedded there does not depend on an ESP fallback loader, which
    // a fresh VM (empty NVRAM) cannot substitute when it is missing. A
    // header that fails to parse keeps the EFI verdict of the magic check.
    let Some(header) = evidence_core::volume::gpt::parse_gpt_header(&gpt_header) else {
        return Ok(VmwareFirmware::Efi);
    };
    Ok(if gpt_has_bios_boot_partition(disk, &header)? {
        VmwareFirmware::Bios
    } else {
        VmwareFirmware::Efi
    })
}

/// GRUB's BIOS boot partition type GUID — the on-disk bytes literally spell
/// "Hah!IdontNeedEFI".
const GPT_BIOS_BOOT_PARTITION: [u8; 16] = *b"Hah!IdontNeedEFI";

/// Sanity bounds for the GPT entry array read: real tables hold a handful of
/// 128-byte entries; anything larger indicates a malformed header, not a
/// legitimate layout.
const MAX_GPT_ENTRY_COUNT: u32 = 4096;
const MAX_GPT_ENTRY_SIZE: u32 = 4096;

fn gpt_has_bios_boot_partition(
    disk: &CowDisk,
    header: &evidence_core::volume::gpt::GptHeader,
) -> Result<bool, EmulationError> {
    let count = header.partition_count.min(MAX_GPT_ENTRY_COUNT);
    let entry_size = header.entry_size.clamp(128, MAX_GPT_ENTRY_SIZE);
    let byte_len = count as usize * entry_size as usize;
    let offset = header.partition_entry_lba * 512;
    if byte_len == 0
        || offset
            .checked_add(byte_len as u64)
            .is_none_or(|end| end > disk.len())
    {
        return Ok(false);
    }
    let mut entries = vec![0u8; byte_len];
    disk.read_exact_at(offset, &mut entries)?;
    Ok(
        evidence_core::volume::gpt::parse_gpt_entries(&entries, entry_size, count)
            .iter()
            .any(|partition| partition.type_guid == GPT_BIOS_BOOT_PARTITION),
    )
}

pub(super) fn image_kind(
    kind: app_services::mount_service::PreparedPhysicalImageKind,
) -> EvidenceImageKind {
    match kind {
        app_services::mount_service::PreparedPhysicalImageKind::E01 => EvidenceImageKind::E01,
        app_services::mount_service::PreparedPhysicalImageKind::Raw => EvidenceImageKind::Raw,
    }
}
