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

mod admission;
mod backend_status;
mod guest;
mod guest_phase;
mod materials;
mod network;
mod prepare;
mod query;
mod recovery_media;
mod session_discovery;
mod session_ops;
mod shutdown;
mod stack;
mod vmware;
mod workspace;

use backend_status::refresh_backend;
use guest::guest_profile_for_source;
use guest_phase::refresh_guest_phase;
pub(crate) use materials::maintenance_tool_available;
use materials::{detect_firmware, image_kind, prepare_machine_materials};
use network::{
    build_maintenance_for_guest, cleanup_prepare_failure, prepare_linux_network_if_needed,
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
    fn lock_error() -> EmulationRegistryError {
        EmulationRegistryError::LockPoisoned
    }
}
#[cfg(test)]
#[path = "../tests/unit/emulation_registry.rs"]
mod tests;
