use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app_services::mount_service::{prepare_emulation_source, MountServiceError};
use domain::{CaseId, DataSourceId};
use evidence_block::{open_block_provider, BlockDeviceError};
use evidence_emulation::{
    CowDisk, CowDiskConfig, EmulationError, ParentIdentity, VmOptions, VmwareFirmware,
};
use thiserror::Error;

use crate::emulation_backend::{self, EmulationBackendHandle};

mod guest;
mod materials;
mod recovery_media;
mod session_discovery;
mod session_ops;
mod vmware;
mod workspace;

use guest::guest_profile_for_source;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmulationGuestPhase {
    Unknown,
    Booting,
    FilesystemMounted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmulationSessionStatus {
    pub session_id: String,
    pub data_source_id: String,
    pub state: EmulationState,
    pub guest_phase: EmulationGuestPhase,
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
    #[error("the data source already has an active emulation session ({session_id})")]
    AlreadyActive { session_id: String },
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
    #[error("emulation bypass failed: {0}")]
    Bypass(#[from] app_services::emulation_bypass::EmulationBypassError),
}

impl transport::ServiceErrorCategory for EmulationRegistryError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::LockPoisoned => transport::ErrorCategory::Internal,
            Self::NotFound(_) | Self::AlreadyActive { .. } => transport::ErrorCategory::Validation,
            Self::Source(error) => error.category(),
            Self::Block(_) | Self::Disk(_) | Self::Workspace(_) => transport::ErrorCategory::Io,
            Self::RecoveryMedia(_) => transport::ErrorCategory::Validation,
            Self::Backend(_) | Self::Vmware(_) => transport::ErrorCategory::External,
            Self::Bypass(error) => error.category(),
        }
    }
}

#[derive(Clone, Default)]
pub struct EmulationRegistry {
    entries: Arc<Mutex<HashMap<String, EmulationEntry>>>,
}

/// Case-scoped references the registry needs to build the service context
/// after resolving the session's data source.
pub struct BypassCaseRef<'a> {
    pub case_conn: &'a rusqlite::Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
}

struct EmulationEntry {
    case_id: String,
    status: EmulationSessionStatus,
    workspace: SessionWorkspace,
    disk: Arc<CowDisk>,
    backend: Option<EmulationBackendHandle>,
    vmware: Option<VmwareControl>,
    boot_started_at: Option<Instant>,
    /// Serializes the long mutating operations (launch, host-side edits,
    /// release) of this session. The global `entries` lock is only ever
    /// taken briefly — to clone this Arc or to update status — and never
    /// held while acquiring this lock, so there is no lock cycle.
    op_lock: Arc<Mutex<()>>,
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
        self.reject_duplicate(case_root, data_source_id)?;
        let recovery_media = recovery_iso
            .map(RecoveryMedia::open)
            .transpose()
            .map_err(|error| EmulationRegistryError::RecoveryMedia(error.to_string()))?;
        let prepared = prepare_emulation_source(case_conn, data_source_id)?;
        let guest = guest_profile_for_source(case_conn, case_root, case_id, data_source_id)?;
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
        // The maintenance CD rides the recovery-media route: Windows installs
        // get the WinPE helper tool, Linux installs get the rescue CD with
        // TARGETS.JSON and the rescue README (no in-guest tool for Linux).
        let maintenance = if recovery_media.is_some() {
            let payload = if guest.is_linux {
                materials::build_linux_rescue_payload(case_conn, case_root, case_id, data_source_id)
            } else {
                build_maintenance_payload(case_conn, case_root, case_id, data_source_id)
            };
            match payload {
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
            materials::MachineSpec {
                firmware,
                guest_os: &guest.guest_os,
                disk_adapter: guest.disk_adapter,
                disk_adapter_reason: &guest.disk_adapter_reason,
            },
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
            guest_phase: EmulationGuestPhase::Unknown,
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
                boot_started_at: None,
                op_lock: Arc::new(Mutex::new(())),
            },
        )?;
        Ok(status)
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
        case_root: &Path,
        data_source_id: &DataSourceId,
    ) -> Result<(), EmulationRegistryError> {
        let active_session = self
            .entries
            .lock()
            .map_err(|_| Self::lock_error())?
            .values()
            .find(|entry| {
                entry.status.data_source_id == data_source_id.0
                    && entry.status.state != EmulationState::Released
            })
            .map(|entry| entry.status.session_id.clone());
        if let Some(session_id) = active_session {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                session_id = %session_id,
                "emulation prepare rejected because this process already owns an active session"
            );
            return Err(EmulationRegistryError::AlreadyActive { session_id });
        }

        // The registry is process-local. A previous application instance may
        // still own a running VM, so consult the durable provenance records
        // before allocating another COW workspace. We never stop a VM here.
        if let Some(session_id) =
            session_discovery::find_active_session(case_root, &data_source_id.0)
                .map_err(|error| EmulationRegistryError::Vmware(error.to_string()))?
        {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                session_id = %session_id,
                "emulation prepare rejected because another process owns an active session"
            );
            return Err(EmulationRegistryError::AlreadyActive { session_id });
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
            return Err(EmulationRegistryError::AlreadyActive {
                session_id: entry.status.session_id.clone(),
            });
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
        let mut failures = Vec::new();
        for id in &ids {
            if let Err(error) = self.release(id) {
                tracing::warn!(session_id = %id, error = %error, "emulation session release failed during cleanup");
                failures.push(format!("{id}: {error}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(EmulationRegistryError::Backend(format!(
                "{} of {} sessions failed to release: {}",
                failures.len(),
                ids.len(),
                failures.join("; ")
            )))
        }
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

fn refresh_backend(entry: &mut EmulationEntry) {
    entry.status.maintenance_media =
        entry.status.maintenance_media && entry.workspace.maintenance_iso_present();
    if matches!(
        entry.status.state,
        EmulationState::Released | EmulationState::FailedCleanupPending | EmulationState::Quiescing
    ) {
        // Quiescing legitimately has its handles checked out by `release`.
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
    refresh_guest_phase(entry);
}

const GUEST_SIGNAL_WINDOW: Duration = Duration::from_secs(90);

fn refresh_guest_phase(entry: &mut EmulationEntry) {
    if entry.status.state != EmulationState::Running
        || entry.status.guest_phase != EmulationGuestPhase::Booting
    {
        return;
    }
    if entry
        .vmware
        .as_ref()
        .is_some_and(VmwareControl::guest_userspace_started)
    {
        entry.status.guest_phase = EmulationGuestPhase::FilesystemMounted;
        entry.boot_started_at = None;
        return;
    }
    if entry
        .boot_started_at
        .is_some_and(|started| started.elapsed() >= GUEST_SIGNAL_WINDOW)
    {
        // VMware Tools is optional and its heartbeat is not a login-ready
        // oracle. Do not leave an unobservable guest labelled "booting"
        // forever once the bounded observation window closes.
        entry.status.guest_phase = EmulationGuestPhase::Unknown;
        entry.boot_started_at = None;
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
            if let Err(error) = self.release(&id) {
                tracing::warn!(session_id = %id, error = %error, "emulation session release failed during registry drop");
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/emulation_registry.rs"]
mod tests;
