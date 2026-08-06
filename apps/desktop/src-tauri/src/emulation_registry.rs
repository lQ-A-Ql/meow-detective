use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use app_services::mount_service::{
    prepare_emulation_source, MountServiceError, PreparedPhysicalImageKind,
};
use domain::{CaseId, DataSourceId};
use evidence_block::{open_block_provider, BlockDeviceError, EvidenceImageKind};
use evidence_emulation::{
    CowDisk, CowDiskConfig, EmulationError, ParentIdentity, VmdkAdapter, VmdkDescriptor,
    VmwareFirmware, VmxConfig,
};
use thiserror::Error;

use crate::emulation_backend::{self, EmulationBackendHandle};

mod recovery_media;
mod vmware;
mod workspace;

use recovery_media::RecoveryMedia;
use vmware::VmwareControl;
use workspace::{EmulationProvenance, RecoveryMediaProvenance, SessionWorkspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmulationState {
    Preparing,
    RecoveringOverlay,
    Mounted,
    DescriptorReady,
    Running,
    Quiescing,
    Sealed,
    Released,
    FailedCleanupPending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmulationSessionStatus {
    pub session_id: String,
    pub data_source_id: String,
    pub state: EmulationState,
    pub logical_length: u64,
    pub error: Option<String>,
}

#[derive(Debug, Error)]
pub enum EmulationRegistryError {
    #[error("emulation registry lock is poisoned")]
    LockPoisoned,
    #[error("emulation session {0} was not found")]
    NotFound(String),
    #[error("the data source already has an active emulation session")]
    AlreadyActive,
    #[error("emulation source validation failed: {0}")]
    Source(#[from] MountServiceError),
    #[error("emulation block provider failed: {0}")]
    Block(#[from] BlockDeviceError),
    #[error("emulation disk failed: {0}")]
    Disk(#[from] EmulationError),
    #[error("emulation workspace failed: {0}")]
    Workspace(String),
    #[error("emulation mount backend failed: {0}")]
    Backend(String),
    #[error("VMware Workstation control failed: {0}")]
    Vmware(String),
    #[error("WinPE recovery media validation failed: {0}")]
    RecoveryMedia(String),
}

impl transport::ServiceErrorCategory for EmulationRegistryError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::LockPoisoned => transport::ErrorCategory::Internal,
            Self::NotFound(_) | Self::AlreadyActive => transport::ErrorCategory::Validation,
            Self::Source(error) => error.category(),
            Self::Block(_) | Self::Disk(_) | Self::Workspace(_) => transport::ErrorCategory::Io,
            Self::RecoveryMedia(_) => transport::ErrorCategory::Validation,
            Self::Backend(_) | Self::Vmware(_) => transport::ErrorCategory::External,
        }
    }
}

#[derive(Clone, Default)]
pub struct EmulationRegistry {
    entries: Arc<Mutex<HashMap<String, EmulationEntry>>>,
}

struct EmulationEntry {
    case_id: String,
    status: EmulationSessionStatus,
    workspace: SessionWorkspace,
    disk: Arc<CowDisk>,
    backend: Option<EmulationBackendHandle>,
    vmware: Option<VmwareControl>,
}

impl EmulationRegistry {
    pub fn prepare_session(
        &self,
        case_conn: &rusqlite::Connection,
        case_root: &Path,
        case_id: &CaseId,
        data_source_id: &DataSourceId,
        recovery_iso: Option<&Path>,
    ) -> Result<EmulationSessionStatus, EmulationRegistryError> {
        self.reject_duplicate(data_source_id)?;
        let recovery_media = recovery_iso
            .map(RecoveryMedia::open)
            .transpose()
            .map_err(|error| EmulationRegistryError::RecoveryMedia(error.to_string()))?;
        let prepared = prepare_emulation_source(case_conn, data_source_id)?;
        let provider = open_block_provider(&prepared.source_path, image_kind(prepared.image_kind))?;
        let identity = ParentIdentity::new(provider.len(), prepared.parent_sha256)?;
        let session_id = format!("emulation-{}", uuid::Uuid::new_v4());
        let workspace = SessionWorkspace::create(case_root, &session_id)
            .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
        let disk = Arc::new(CowDisk::create(
            workspace.overlay_path(),
            provider,
            identity.clone(),
            CowDiskConfig::default(),
        )?);
        let firmware = detect_firmware(&disk)?;
        let backend =
            emulation_backend::start(Arc::clone(&disk), workspace.root(), workspace.mount_point())
                .map_err(|error| EmulationRegistryError::Backend(error.to_string()))?;
        let materials = prepare_machine_materials(
            &workspace,
            &identity,
            firmware,
            case_id,
            data_source_id,
            &session_id,
            recovery_media.as_ref(),
        );
        if let Err(error) = materials {
            let _ = backend.stop();
            return Err(error);
        }
        let status = EmulationSessionStatus {
            session_id: session_id.clone(),
            data_source_id: data_source_id.0.clone(),
            state: EmulationState::DescriptorReady,
            logical_length: identity.logical_length(),
            error: None,
        };
        self.insert_entry(
            session_id,
            EmulationEntry {
                case_id: case_id.0.clone(),
                status: status.clone(),
                workspace,
                disk,
                backend: Some(backend),
                vmware: None,
            },
        )?;
        Ok(status)
    }

    pub fn launch(
        &self,
        session_id: &str,
    ) -> Result<EmulationSessionStatus, EmulationRegistryError> {
        let vmx_path = {
            let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
            let entry = entries
                .get_mut(session_id)
                .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
            refresh_backend(entry);
            if entry.status.state != EmulationState::DescriptorReady {
                return Err(EmulationRegistryError::Vmware(
                    "session is not ready for launch".to_string(),
                ));
            }
            entry.workspace.vmx_path().to_path_buf()
        };
        let control = vmware::launch(&vmx_path)
            .map_err(|error| EmulationRegistryError::Vmware(error.to_string()))?;
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get_mut(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        entry.vmware = Some(control);
        entry.status.state = EmulationState::Running;
        Ok(entry.status.clone())
    }

    pub fn release(
        &self,
        session_id: &str,
    ) -> Result<EmulationSessionStatus, EmulationRegistryError> {
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get_mut(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        if entry.status.state == EmulationState::Released {
            return Ok(entry.status.clone());
        }
        entry.status.state = EmulationState::Quiescing;
        match quiesce_entry(entry) {
            Ok(()) => {
                entry.status.state = EmulationState::Sealed;
                entry.status.state = EmulationState::Released;
                entry.status.error = None;
                Ok(entry.status.clone())
            }
            Err(error) => {
                entry.status.state = EmulationState::FailedCleanupPending;
                entry.status.error = Some(error.to_string());
                Err(error)
            }
        }
    }

    pub fn status(
        &self,
        session_id: &str,
    ) -> Result<EmulationSessionStatus, EmulationRegistryError> {
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get_mut(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        refresh_backend(entry);
        Ok(entry.status.clone())
    }

    pub fn list(&self) -> Result<Vec<EmulationSessionStatus>, EmulationRegistryError> {
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        for entry in entries.values_mut() {
            refresh_backend(entry);
        }
        let mut statuses = entries
            .values()
            .map(|entry| entry.status.clone())
            .collect::<Vec<_>>();
        statuses.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        Ok(statuses)
    }

    pub fn cleanup_case(&self, case_id: &str) -> Result<(), EmulationRegistryError> {
        self.cleanup_matching(|entry| entry.case_id == case_id)
    }

    pub fn cleanup_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), EmulationRegistryError> {
        self.cleanup_matching(|entry| {
            entry.case_id == case_id && entry.status.data_source_id == data_source_id
        })
    }

    fn reject_duplicate(
        &self,
        data_source_id: &DataSourceId,
    ) -> Result<(), EmulationRegistryError> {
        let active = self
            .entries
            .lock()
            .map_err(|_| Self::lock_error())?
            .values()
            .any(|entry| {
                entry.status.data_source_id == data_source_id.0
                    && entry.status.state != EmulationState::Released
            });
        if active {
            return Err(EmulationRegistryError::AlreadyActive);
        }
        Ok(())
    }

    fn insert_entry(
        &self,
        session_id: String,
        entry: EmulationEntry,
    ) -> Result<(), EmulationRegistryError> {
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        if entries.values().any(|current| {
            current.status.data_source_id == entry.status.data_source_id
                && current.status.state != EmulationState::Released
        }) {
            drop(entries);
            if let Some(backend) = entry.backend.as_ref() {
                let _ = backend.stop();
            }
            return Err(EmulationRegistryError::AlreadyActive);
        }
        entries.insert(session_id, entry);
        Ok(())
    }

    fn cleanup_matching(
        &self,
        predicate: impl Fn(&EmulationEntry) -> bool,
    ) -> Result<(), EmulationRegistryError> {
        let ids = self
            .entries
            .lock()
            .map_err(|_| Self::lock_error())?
            .iter()
            .filter(|(_, entry)| predicate(entry) && entry.status.state != EmulationState::Released)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.release(&id)?;
        }
        Ok(())
    }

    fn lock_error() -> EmulationRegistryError {
        EmulationRegistryError::LockPoisoned
    }
}

fn prepare_machine_materials(
    workspace: &SessionWorkspace,
    identity: &ParentIdentity,
    firmware: VmwareFirmware,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    session_id: &str,
    recovery_media: Option<&RecoveryMedia>,
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
    let mut vmx = VmxConfig::new("disk.vmdk", firmware)?.with_disk_adapter(disk_adapter);
    if let Some(media) = recovery_media {
        vmx = vmx.with_recovery_iso(media.vmware_path())?;
    }
    let rendered_vmx = vmx.render();
    VmxConfig::validate_rendered(&rendered_vmx)?;
    workspace
        .write_vmx(&rendered_vmx)
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
    workspace
        .write_provenance(&EmulationProvenance::new(
            session_id,
            &case_id.0,
            &data_source_id.0,
            identity,
            firmware,
            recovery_media.map(|media| RecoveryMediaProvenance {
                file_name: media.file_name(),
                length: media.length(),
                sha256: media.sha256(),
            }),
        ))
        .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))
}

fn detect_firmware(disk: &CowDisk) -> Result<VmwareFirmware, EmulationRegistryError> {
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

fn image_kind(kind: PreparedPhysicalImageKind) -> EvidenceImageKind {
    match kind {
        PreparedPhysicalImageKind::E01 => EvidenceImageKind::E01,
        PreparedPhysicalImageKind::Raw => EvidenceImageKind::Raw,
    }
}

fn quiesce_entry(entry: &mut EmulationEntry) -> Result<(), EmulationRegistryError> {
    if let Some(control) = entry.vmware.as_ref() {
        control
            .stop_bounded()
            .map_err(|error| EmulationRegistryError::Vmware(error.to_string()))?;
        entry.vmware = None;
    }
    entry.disk.flush()?;
    if let Some(backend) = entry.backend.as_ref() {
        backend
            .stop()
            .map_err(|error| EmulationRegistryError::Backend(error.to_string()))?;
    }
    entry.backend = None;
    Ok(())
}

fn refresh_backend(entry: &mut EmulationEntry) {
    if matches!(
        entry.status.state,
        EmulationState::Released | EmulationState::FailedCleanupPending
    ) {
        return;
    }
    let Some(backend) = entry.backend.as_ref() else {
        entry.status.state = EmulationState::FailedCleanupPending;
        entry.status.error = Some("emulation mount backend handle is missing".to_string());
        return;
    };
    match backend.poll_exit() {
        Ok(None) => {}
        Ok(Some(error)) => {
            entry.status.state = EmulationState::FailedCleanupPending;
            entry.status.error = Some(error);
        }
        Err(error) => {
            entry.status.state = EmulationState::FailedCleanupPending;
            entry.status.error = Some(error.to_string());
        }
    }
}

impl Drop for EmulationRegistry {
    fn drop(&mut self) {
        if Arc::strong_count(&self.entries) != 1 {
            return;
        }
        let ids = self
            .entries
            .lock()
            .map(|entries| entries.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for id in ids {
            let _ = self.release(&id);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/emulation_registry.rs"]
mod tests;
