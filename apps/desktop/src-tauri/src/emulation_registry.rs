use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use app_services::mount_service::{prepare_emulation_source, MountServiceError};
use domain::{CaseId, DataSourceId};
use evidence_block::{open_block_provider, BlockDeviceError};
use evidence_emulation::{
    CowDisk, CowDiskConfig, EmulationError, ParentIdentity, VmOptions, VmwareFirmware,
};
use thiserror::Error;

use crate::emulation_backend::{self, EmulationBackendHandle};

mod materials;
mod recovery_media;
mod vmware;
mod workspace;

pub(crate) use materials::maintenance_tool_available;
use materials::{
    build_maintenance_payload, detect_firmware, image_kind, prepare_machine_materials,
};
use recovery_media::RecoveryMedia;
use vmware::VmwareControl;
use workspace::{ProvenanceIds, SessionWorkspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmulationState {
    DescriptorReady,
    Running,
    Quiescing,
    Released,
    FailedCleanupPending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmulationSessionStatus {
    pub session_id: String,
    pub data_source_id: String,
    pub state: EmulationState,
    pub logical_length: u64,
    pub maintenance_media: bool,
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
        options: VmOptions,
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
        let (disk, firmware, backend) = match build_session_disk(&workspace, provider, &identity) {
            Ok(parts) => parts,
            Err(error) => {
                workspace.remove_best_effort();
                return Err(error);
            }
        };
        // The maintenance CD only makes sense on the PE boot route; building
        // it needs the import index, so it runs before material generation
        // and shares the same rollback path.
        let maintenance = if recovery_media.is_some() {
            match build_maintenance_payload(case_conn, case_root, case_id, data_source_id) {
                Ok(payload) => payload,
                Err(error) => {
                    let _ = backend.stop();
                    workspace.remove_best_effort();
                    return Err(error);
                }
            }
        } else {
            None
        };
        let materials = prepare_machine_materials(
            &workspace,
            &identity,
            firmware,
            ProvenanceIds {
                session_id: &session_id,
                case_id: &case_id.0,
                data_source_id: &data_source_id.0,
            },
            recovery_media.as_ref(),
            options,
            maintenance.as_ref(),
        );
        if let Err(error) = materials {
            let _ = backend.stop();
            workspace.remove_best_effort();
            return Err(error);
        }
        let status = EmulationSessionStatus {
            session_id: session_id.clone(),
            data_source_id: data_source_id.0.clone(),
            state: EmulationState::DescriptorReady,
            logical_length: identity.logical_length(),
            maintenance_media: maintenance.is_some(),
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
            entry.workspace.remove_best_effort();
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

fn build_session_disk(
    workspace: &SessionWorkspace,
    provider: Arc<dyn evidence_block::BlockProvider>,
    identity: &ParentIdentity,
) -> Result<(Arc<CowDisk>, VmwareFirmware, EmulationBackendHandle), EmulationRegistryError> {
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
    Ok((disk, firmware, backend))
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
    entry.status.maintenance_media =
        entry.status.maintenance_media && entry.workspace.maintenance_iso_present();
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
