//! Session operations: launch, host-side edits and release.
//!
//! All long mutating operations are serialized per session through the
//! entry's `op_lock`. The global `entries` lock is only taken briefly (to
//! clone the Arc or to update status) and is never held while acquiring the
//! op-lock, so there is no lock cycle. Once the op guard is held, no other
//! operation can flip the session state, which is why the state is
//! re-verified after the guard is acquired rather than trusted from before.
//!
//! Workspace retention policy: the overlay, generated VM materials and the
//! maintenance ISO are single-session disposable artifacts. A release that
//! reaches `Released` therefore removes the session workspace best-effort.
//! A release that fails leaves the session in `FailedCleanupPending` with
//! the workspace intact so a retry can attempt the full stop/flush/unmount
//! sequence again and the artifacts remain available for inspection.

use std::sync::{Arc, Mutex};

use domain::DataSourceId;
use evidence_emulation::CowDisk;

use super::vmware::{self, VmwareControl};
use super::{
    refresh_backend, BypassCaseRef, EmulationRegistry, EmulationRegistryError,
    EmulationSessionStatus, EmulationState,
};
use crate::emulation_backend::EmulationBackendHandle;

/// A session resolved for a host-side edit: disk, data source and op-lock.
struct EditSession {
    disk: Arc<CowDisk>,
    data_source_id: String,
    op_lock: Arc<Mutex<()>>,
}

impl EmulationRegistry {
    pub fn launch(
        &self,
        session_id: &str,
    ) -> Result<EmulationSessionStatus, EmulationRegistryError> {
        let (op_lock, vmx_path) = {
            let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
            let entry = entries
                .get_mut(session_id)
                .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
            Self::require_usable_disk(&entry.disk)?;
            refresh_backend(entry);
            if entry.status.state != EmulationState::DescriptorReady {
                return Err(EmulationRegistryError::Vmware(
                    "session is not ready for launch".to_string(),
                ));
            }
            (
                Arc::clone(&entry.op_lock),
                entry.workspace.vmx_path().to_path_buf(),
            )
        };
        let _op_guard = op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let control = vmware::launch(&vmx_path)
            .map_err(|error| EmulationRegistryError::Vmware(error.to_string()))?;
        // The op guard is still held, so no release or edit could have
        // interleaved with the launch.
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get_mut(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        entry.vmware = Some(control);
        entry.status.state = EmulationState::Running;
        Ok(entry.status.clone())
    }

    pub fn apply_bypass(
        &self,
        case: &BypassCaseRef<'_>,
        session_id: &str,
        partition_index: u32,
        rid: u32,
        action: transport::dto::EmulationBypassActionDto,
    ) -> Result<transport::dto::EmulationBypassResultDto, EmulationRegistryError> {
        let session = self.edit_session_entry(
            session_id,
            "bypass edits are only allowed before the guest is launched",
        )?;
        let _op_guard = session.op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let mut result = app_services::emulation_bypass::apply_bypass(
            &session.disk,
            &app_services::emulation_bypass::BypassCaseContext {
                case_conn: case.case_conn,
                case_root: case.case_root,
                case_id: case.case_id,
                data_source_id: &DataSourceId(session.data_source_id),
            },
            partition_index,
            rid,
            action,
        )?;
        result.session_id = session_id.to_string();
        Ok(result)
    }

    pub fn cleanup_osdata(
        &self,
        case: &BypassCaseRef<'_>,
        session_id: &str,
        partition_index: u32,
    ) -> Result<transport::dto::EmulationOsdataCleanupDto, EmulationRegistryError> {
        let session = self.edit_session_entry(
            session_id,
            "namespace edits are only allowed before the guest is launched",
        )?;
        let _op_guard = session.op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let mut result = app_services::emulation_osdata::cleanup_osdata(
            &session.disk,
            &app_services::emulation_bypass::BypassCaseContext {
                case_conn: case.case_conn,
                case_root: case.case_root,
                case_id: case.case_id,
                data_source_id: &DataSourceId(session.data_source_id),
            },
            partition_index,
        )?;
        result.session_id = session_id.to_string();
        Ok(result)
    }

    pub fn apply_linux_bypass(
        &self,
        case: &BypassCaseRef<'_>,
        session_id: &str,
        partition_index: u32,
        username: &str,
    ) -> Result<transport::dto::EmulationLinuxBypassResultDto, EmulationRegistryError> {
        let session = self.edit_session_entry(
            session_id,
            "bypass edits are only allowed before the guest is launched",
        )?;
        let _op_guard = session.op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let mut result = app_services::emulation_linux_bypass::apply_linux_bypass(
            &session.disk,
            &app_services::emulation_bypass::BypassCaseContext {
                case_conn: case.case_conn,
                case_root: case.case_root,
                case_id: case.case_id,
                data_source_id: &DataSourceId(session.data_source_id),
            },
            partition_index,
            username,
        )?;
        result.session_id = session_id.to_string();
        Ok(result)
    }

    /// Install the UEFI fallback boot path (`\EFI\BOOT\BOOTX64.EFI`) into the
    /// session overlay. Pure disk edit: no case database access is needed.
    pub fn install_efi_fallback(
        &self,
        session_id: &str,
    ) -> Result<transport::dto::EmulationEfiFallbackResultDto, EmulationRegistryError> {
        let session = self.edit_session_entry(
            session_id,
            "ESP edits are only allowed before the guest is launched",
        )?;
        let _op_guard = session.op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let mut result = app_services::emulation_efi_fallback::install_efi_fallback(
            &session.disk,
            &session.data_source_id,
        )?;
        result.session_id = session_id.to_string();
        Ok(result)
    }

    /// Assess and repair dirty XFS logs through the session overlay. Needs
    /// the case context because the volume layout comes from the source
    /// catalog.
    pub fn repair_fs_journals(
        &self,
        case: &BypassCaseRef<'_>,
        session_id: &str,
    ) -> Result<transport::dto::EmulationFsRepairResultDto, EmulationRegistryError> {
        let session = self.edit_session_entry(
            session_id,
            "filesystem repairs are only allowed before the guest is launched",
        )?;
        let _op_guard = session.op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let mut result = app_services::emulation_fs_repair::repair_xfs_logs(
            &session.disk,
            &app_services::emulation_bypass::BypassCaseContext {
                case_conn: case.case_conn,
                case_root: case.case_root,
                case_id: case.case_id,
                data_source_id: &DataSourceId(session.data_source_id),
            },
        )?;
        result.session_id = session_id.to_string();
        Ok(result)
    }

    /// Resolve a session in `DescriptorReady` for a host-side edit.
    fn edit_session_entry(
        &self,
        session_id: &str,
        state_error: &str,
    ) -> Result<EditSession, EmulationRegistryError> {
        let entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        if entry.status.state != EmulationState::DescriptorReady {
            return Err(EmulationRegistryError::Vmware(state_error.to_string()));
        }
        Ok(EditSession {
            disk: Arc::clone(&entry.disk),
            data_source_id: entry.status.data_source_id.clone(),
            op_lock: Arc::clone(&entry.op_lock),
        })
    }

    fn require_state(
        &self,
        session_id: &str,
        expected: EmulationState,
    ) -> Result<(), EmulationRegistryError> {
        let entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        if entry.status.state != expected {
            return Err(EmulationRegistryError::Vmware(format!(
                "session is no longer in the {expected:?} state"
            )));
        }
        if expected == EmulationState::DescriptorReady {
            Self::require_usable_disk(&entry.disk)?;
        }
        Ok(())
    }

    fn require_usable_disk(disk: &CowDisk) -> Result<(), EmulationRegistryError> {
        if disk.is_poisoned() {
            return Err(EmulationRegistryError::Disk(
                evidence_emulation::EmulationError::CorruptOverlay(
                    "overlay session must be released and recreated after a failed write"
                        .to_string(),
                ),
            ));
        }
        Ok(())
    }

    pub fn release(
        &self,
        session_id: &str,
    ) -> Result<EmulationSessionStatus, EmulationRegistryError> {
        let op_lock = {
            let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
            let entry = entries
                .get_mut(session_id)
                .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
            if entry.status.state == EmulationState::Released {
                return Ok(entry.status.clone());
            }
            Arc::clone(&entry.op_lock)
        };
        let _op_guard = op_lock.lock().map_err(|_| Self::lock_error())?;
        // Take the resources and mark Quiescing while holding the op guard;
        // launch and host-side edits are serialized behind the same guard.
        let (vmware, disk, backend) = {
            let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
            let entry = entries
                .get_mut(session_id)
                .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
            if entry.status.state == EmulationState::Released {
                return Ok(entry.status.clone());
            }
            entry.status.state = EmulationState::Quiescing;
            (
                entry.vmware.take(),
                Arc::clone(&entry.disk),
                entry.backend.take(),
            )
        };
        // The long stop/flush/unmount sequence runs without the global lock.
        let outcome = quiesce_resources(vmware.as_ref(), &disk, backend.as_ref());
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get_mut(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        match outcome {
            Ok(()) => {
                entry.status.state = EmulationState::Released;
                entry.status.error = None;
                let status = entry.status.clone();
                // The overlay and materials are single-session disposable
                // artifacts; the backend is stopped, so drop the workspace.
                entry.workspace.remove_best_effort();
                Ok(status)
            }
            Err(error) => {
                // Put the handles back so a retry attempts the full sequence
                // again; both stop paths are idempotent.
                if entry.vmware.is_none() {
                    entry.vmware = vmware;
                }
                if entry.backend.is_none() {
                    entry.backend = backend;
                }
                entry.status.state = EmulationState::FailedCleanupPending;
                entry.status.error = Some(error.to_string());
                Err(error)
            }
        }
    }
}

fn quiesce_resources(
    vmware: Option<&VmwareControl>,
    disk: &Arc<CowDisk>,
    backend: Option<&EmulationBackendHandle>,
) -> Result<(), EmulationRegistryError> {
    if let Some(control) = vmware {
        control
            .stop_bounded()
            .map_err(|error| EmulationRegistryError::Vmware(error.to_string()))?;
    }
    if let Err(error) = disk.flush() {
        // A poisoned overlay cannot flush by design; the workspace is
        // disposable either way, so do not trap the session in
        // FailedCleanupPending over an unwinnable retry.
        if disk.is_poisoned() {
            tracing::warn!(error = %error, "release: overlay is poisoned, skipping flush");
        } else {
            return Err(error.into());
        }
    }
    if let Some(backend) = backend {
        backend
            .stop()
            .map_err(|error| EmulationRegistryError::Backend(error.to_string()))?;
    }
    Ok(())
}
