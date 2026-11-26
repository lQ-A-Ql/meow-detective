//! Machine material generation: VMDK descriptor, VMX configuration,
//! provenance, and the optional maintenance CD-ROM that carries the WinPE
//! helper tool plus the import-index-derived target manifest into the guest.

use std::io::Read;
use std::path::{Path, PathBuf};

use evidence_emulation::{
    build_iso, CowDisk, EmulationError, IsoFile, ParentIdentity, VmOptions, VmdkAdapter,
    VmdkDescriptor, VmwareFirmware, VmxConfig,
};

use super::recovery_media::RecoveryMedia;
use super::workspace::{
    EmulationProvenance, ProvenanceGuest, ProvenanceIds, RecoveryMediaProvenance, SessionWorkspace,
};
use super::EmulationRegistryError;

/// The helper executable is delivered read-only inside the guest; anything
/// larger indicates a packaging mistake rather than a legitimate build.
const MAX_MAINTENANCE_TOOL_BYTES: u64 = 64 * 1024 * 1024;
const DERIVED_RECOVERY_README: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/emulation/derived-ntfs-recovery.txt"
));

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

pub(super) struct MaintenancePayload {
    pub tool: Option<Vec<u8>>,
    pub targets_json: Vec<u8>,
    pub readme: &'static str,
}

pub(super) fn build_maintenance_payload(
    preflight: &transport::dto::EmulationPreflightDto,
    include_without_tool: bool,
) -> Result<Option<MaintenancePayload>, EmulationRegistryError> {
    let tool = resolve_maintenance_tool()
        .map(|path| read_maintenance_tool(&path))
        .transpose()?;
    if tool.is_none() && !include_without_tool {
        return Ok(None);
    }
    let targets_json = serde_json::to_vec_pretty(&preflight)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    Ok(Some(MaintenancePayload {
        readme: if tool.is_some() {
            MAINTENANCE_README
        } else {
            DERIVED_RECOVERY_README
        },
        tool,
        targets_json,
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
    if let Some(path) = crate::embedded_maintenance::path() {
        return Some(path);
    }
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
    let dev = manifest_dir.join("../../../target/release").join(file_name);
    dev.is_file().then_some(dev)
}

/// The guest identity the VMX/VMDK pair is rendered for.
pub(super) struct MachineSpec<'a> {
    pub firmware: VmwareFirmware,
    pub guest_os: &'a str,
    pub disk_adapter: VmdkAdapter,
    pub disk_adapter_reason: &'a str,
    pub network_pci_slot: u16,
    pub derived_ntfs: Option<&'a transport::dto::case::DerivedNtfsDiskDto>,
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
    // The caller picks the adapter from the guest profile. This is important
    // for Linux images whose initramfs only carries ata_piix or only carries
    // the VMware/LSI SCSI modules.
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
        .with_linux_network_interface_index(spec.network_pci_slot)
        .with_options(options)?;
    if let Some(media) = recovery_media {
        vmx = vmx.with_recovery_iso(media.vmware_path())?;
    }
    if let Some(payload) = maintenance {
        let iso_path = write_guest_maintenance(workspace, payload, spec.derived_ntfs)?;
        let iso_text = super::recovery_media::vmware_path(&iso_path)
            .map_err(|error| EmulationRegistryError::RecoveryMedia(error.to_string()))?;
        vmx = vmx.with_maintenance_iso(&iso_text)?;
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
            ProvenanceGuest {
                firmware: spec.firmware,
                options,
                disk_adapter,
                disk_adapter_reason: spec.disk_adapter_reason,
                maintenance_media: maintenance.is_some(),
                derived_ntfs: spec.derived_ntfs,
            },
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
    let Some(offset) = header.partition_entry_lba.checked_mul(512) else {
        return Ok(false);
    };
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

fn write_guest_maintenance(
    workspace: &SessionWorkspace,
    payload: &MaintenancePayload,
    derived: Option<&transport::dto::case::DerivedNtfsDiskDto>,
) -> Result<PathBuf, EmulationRegistryError> {
    let derived_json = derived
        .map(serde_json::to_vec_pretty)
        .transpose()
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    let mut files = vec![
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
        files.push(IsoFile {
            name: "MEOWMTN.EXE",
            data: tool,
        });
    }
    if let Some(json) = &derived_json {
        files.push(IsoFile {
            name: "DERIVED.JSON",
            data: json,
        });
        files.push(IsoFile {
            name: "DERIVED.TXT",
            data: DERIVED_RECOVERY_README.as_bytes(),
        });
    }
    let iso = build_iso(&files)?;
    workspace
        .write_maintenance_iso(&iso)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))
}

fn read_maintenance_tool(path: &Path) -> Result<Vec<u8>, EmulationRegistryError> {
    let file = std::fs::File::open(path)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    let metadata = file
        .metadata()
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    if metadata.len() > MAX_MAINTENANCE_TOOL_BYTES {
        return Err(EmulationRegistryError::Workspace(
            "maintenance tool exceeds the delivery size limit".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_MAINTENANCE_TOOL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    if bytes.len() as u64 > MAX_MAINTENANCE_TOOL_BYTES {
        return Err(EmulationRegistryError::Workspace(
            "maintenance tool exceeds the delivery size limit".into(),
        ));
    }
    Ok(bytes)
}
