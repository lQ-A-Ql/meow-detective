use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use evidence_emulation::{ParentIdentity, VmOptions, VmwareFirmware};
use serde::Serialize;
use thiserror::Error;

const EXTENT_READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub(super) enum WorkspaceError {
    #[error("derived workspace I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("derived workspace escaped the active case")]
    EscapedCase,
    #[error("derived workspace contains a reparse point")]
    ReparsePoint,
    #[error("mounted disk.raw did not become available")]
    ExtentUnavailable,
    #[error("derived material already exists")]
    MaterialExists,
    #[error("provenance serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub(super) struct SessionWorkspace {
    root: PathBuf,
    mount: PathBuf,
    overlay: PathBuf,
    vmdk: PathBuf,
    vmx: PathBuf,
    provenance: PathBuf,
    maintenance_iso: PathBuf,
}

impl SessionWorkspace {
    pub(super) fn create(case_root: &Path, session_id: &str) -> Result<Self, WorkspaceError> {
        let canonical_case = case_root.canonicalize()?;
        let base = canonical_case.join("emulation");
        if !base.exists() {
            fs::create_dir(&base)?;
        }
        validate_owned_directory(&canonical_case, &base)?;
        let directory_name = session_id
            .strip_prefix("emulation-")
            .ok_or(WorkspaceError::EscapedCase)?;
        let directory_name = uuid::Uuid::parse_str(directory_name)
            .map_err(|_| WorkspaceError::EscapedCase)?
            .to_string();
        let root = base.join(directory_name);
        fs::create_dir(&root)?;
        let mount = root.join("mount");
        fs::create_dir(&mount)?;
        Ok(Self {
            overlay: root.join("overlay.cow"),
            vmdk: root.join("disk.vmdk"),
            vmx: root.join("machine.vmx"),
            provenance: root.join("provenance.json"),
            maintenance_iso: root.join("maintenance.iso"),
            root,
            mount,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn mount_point(&self) -> &Path {
        &self.mount
    }

    pub(super) fn overlay_path(&self) -> &Path {
        &self.overlay
    }

    pub(super) fn vmx_path(&self) -> &Path {
        &self.vmx
    }

    pub(super) fn extent_length(&self) -> Result<u64, WorkspaceError> {
        let extent = self.mount.join("disk.raw");
        let deadline = std::time::Instant::now() + EXTENT_READY_TIMEOUT;
        loop {
            if let Ok(metadata) = fs::metadata(&extent) {
                return Ok(metadata.len());
            }
            if std::time::Instant::now() >= deadline {
                return Err(WorkspaceError::ExtentUnavailable);
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    pub(super) fn write_vmdk(&self, value: &str) -> Result<(), WorkspaceError> {
        atomic_write(&self.vmdk, value.as_bytes())
    }

    pub(super) fn write_vmx(&self, value: &str) -> Result<(), WorkspaceError> {
        atomic_write(&self.vmx, value.as_bytes())
    }

    pub(super) fn write_provenance(
        &self,
        value: &EmulationProvenance<'_>,
    ) -> Result<(), WorkspaceError> {
        let json = serde_json::to_vec_pretty(value)?;
        atomic_write(&self.provenance, &json)
    }

    /// Writes the generated maintenance CD image and returns its absolute
    /// path for the VMX attachment.
    pub(super) fn write_maintenance_iso(&self, bytes: &[u8]) -> Result<PathBuf, WorkspaceError> {
        atomic_write(&self.maintenance_iso, bytes)?;
        Ok(self.maintenance_iso.clone())
    }

    pub(super) fn maintenance_iso_present(&self) -> bool {
        self.maintenance_iso.is_file()
    }

    /// Removes the whole session directory after a failed prepare. Best
    /// effort: a mounted backend must already be stopped by the caller, and a
    /// removal failure is logged rather than propagated.
    pub(super) fn remove_best_effort(self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            tracing::warn!(
                error = %error,
                path = %self.root.display(),
                "emulation workspace rollback removal failed"
            );
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EmulationProvenance<'a> {
    schema_version: u32,
    session_id: &'a str,
    case_id: &'a str,
    data_source_id: &'a str,
    parent_sha256: String,
    logical_length: u64,
    firmware: &'static str,
    created_at: String,
    evidence_access: &'static str,
    guest_network: &'static str,
    guest_clipboard: bool,
    guest_time_sync: bool,
    maintenance_media: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_media: Option<RecoveryMediaProvenance<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecoveryMediaProvenance<'a> {
    pub(super) file_name: &'a str,
    pub(super) length: u64,
    pub(super) sha256: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ProvenanceIds<'a> {
    pub session_id: &'a str,
    pub case_id: &'a str,
    pub data_source_id: &'a str,
}

impl<'a> EmulationProvenance<'a> {
    pub(super) fn new(
        ids: ProvenanceIds<'a>,
        identity: &ParentIdentity,
        firmware: VmwareFirmware,
        options: VmOptions,
        maintenance_media: bool,
        recovery_media: Option<RecoveryMediaProvenance<'a>>,
    ) -> Self {
        Self {
            schema_version: 1,
            session_id: ids.session_id,
            case_id: ids.case_id,
            data_source_id: ids.data_source_id,
            parent_sha256: encode_hex(identity.sha256()),
            logical_length: identity.logical_length(),
            firmware: match firmware {
                VmwareFirmware::Bios => "bios",
                VmwareFirmware::Efi => "efi",
            },
            created_at: chrono::Utc::now().to_rfc3339(),
            evidence_access: "read-only-parent-with-application-cow",
            guest_network: if options.network {
                "host-only"
            } else {
                "disabled"
            },
            guest_clipboard: options.clipboard,
            guest_time_sync: options.time_sync,
            maintenance_media,
            recovery_media,
        }
    }
}

fn validate_owned_directory(parent: &Path, child: &Path) -> Result<(), WorkspaceError> {
    let canonical = child.canonicalize()?;
    if canonical.parent() != Some(parent) {
        return Err(WorkspaceError::EscapedCase);
    }
    if is_reparse_point(&fs::symlink_metadata(child)?) {
        return Err(WorkspaceError::ReparsePoint);
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), WorkspaceError> {
    if path.exists() {
        return Err(WorkspaceError::MaterialExists);
    }
    let temporary = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & winapi::um::winnt::FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
#[path = "../../tests/unit/emulation_registry/workspace.rs"]
mod tests;
