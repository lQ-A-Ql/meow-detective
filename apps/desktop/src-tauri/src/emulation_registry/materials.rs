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
   The tool reads TARGETS.JSON, verifies the expected Windows installation,\r\n\
   removes a leftover OSDATA node when present, and can apply or restore the\r\n\
   Utilman logon bypass on request (see MEOWMTN.EXE usage output).\r\n\
\r\n\
All writes land on the copy-on-write overlay; the evidence image is never\r\n\
modified.\r\n";

pub(super) struct MaintenancePayload {
    pub tool: Vec<u8>,
    pub targets_json: Vec<u8>,
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
    Ok(Some(MaintenancePayload { tool, targets_json }))
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

pub(super) fn prepare_machine_materials(
    workspace: &SessionWorkspace,
    identity: &ParentIdentity,
    firmware: VmwareFirmware,
    ids: ProvenanceIds<'_>,
    recovery_media: Option<&RecoveryMedia>,
    options: VmOptions,
    maintenance: Option<&MaintenancePayload>,
) -> Result<(), EmulationRegistryError> {
    // Windows and the user-selected PE must not depend on an optional LSI
    // Logic driver; the inbox IDE path keeps both boot routes enumerable.
    let disk_adapter = VmdkAdapter::Ide;
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
    let mut vmx = VmxConfig::new("disk.vmdk", firmware)?
        .with_disk_adapter(disk_adapter)
        .with_options(options);
    if let Some(media) = recovery_media {
        vmx = vmx.with_recovery_iso(media.vmware_path())?;
    }
    if let Some(payload) = maintenance {
        let iso = build_iso(&[
            IsoFile {
                name: "MEOWMTN.EXE",
                data: &payload.tool,
            },
            IsoFile {
                name: "TARGETS.JSON",
                data: &payload.targets_json,
            },
            IsoFile {
                name: "README.TXT",
                data: MAINTENANCE_README.as_bytes(),
            },
        ])?;
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
            firmware,
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
    Ok(if &gpt_header[..8] == b"EFI PART" {
        VmwareFirmware::Efi
    } else {
        VmwareFirmware::Bios
    })
}

pub(super) fn image_kind(
    kind: app_services::mount_service::PreparedPhysicalImageKind,
) -> EvidenceImageKind {
    match kind {
        app_services::mount_service::PreparedPhysicalImageKind::E01 => EvidenceImageKind::E01,
        app_services::mount_service::PreparedPhysicalImageKind::Raw => EvidenceImageKind::Raw,
    }
}
