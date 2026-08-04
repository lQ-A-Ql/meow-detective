use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use app_services::mount_service::prepare_mount_session;
use evidence_mount::MountSession;
use thiserror::Error;
use transport::dto::{MountModeDto, MountStateDto, MountStatusDto, MountTargetDto};

use crate::mount_backend::{self, BackendHandle, MountBackendError};

#[derive(Clone)]
pub struct MountRegistry {
    entries: Arc<Mutex<HashMap<String, MountEntry>>>,
}

struct MountEntry {
    case_id: String,
    session: MountSession,
    status: MountStatusDto,
    backend: Option<BackendHandle>,
}

#[derive(Debug, Error)]
pub(crate) enum MountRegistryError {
    #[error("mount registry lock is poisoned")]
    LockPoisoned,
    #[error("mount {0} was not found")]
    NotFound(String),
    #[error("the data source partition is already mounted")]
    AlreadyMounted,
    #[error("mount backend error: {0}")]
    Backend(#[from] MountBackendError),
    #[error("mount session error: {0}")]
    Session(#[from] app_services::mount_service::MountServiceError),
}

impl transport::ServiceErrorCategory for MountRegistryError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::LockPoisoned => transport::ErrorCategory::Internal,
            Self::Session(error) => error.category(),
            Self::NotFound(_) | Self::AlreadyMounted => transport::ErrorCategory::Validation,
            #[cfg(not(windows))]
            Self::Backend(MountBackendError::UnsupportedPlatform) => {
                transport::ErrorCategory::Unsupported
            }
            Self::Backend(MountBackendError::InvalidMountPoint(_)) => {
                transport::ErrorCategory::Validation
            }
            Self::Backend(MountBackendError::StartupTimeout) => transport::ErrorCategory::Timeout,
            Self::Backend(MountBackendError::Backend(_)) => transport::ErrorCategory::External,
        }
    }
}

impl Default for MountRegistry {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Drop for MountRegistry {
    fn drop(&mut self) {
        if Arc::strong_count(&self.entries) == 1 {
            let _ = self.cleanup_all();
        }
    }
}

impl MountRegistry {
    pub(crate) fn mount(
        &self,
        case_conn: &rusqlite::Connection,
        case_root: &std::path::Path,
        case_id: &domain::CaseId,
        data_source_id: &domain::DataSourceId,
        partition_index: usize,
        requested_mount_point: Option<&str>,
    ) -> Result<MountStatusDto, MountRegistryError> {
        let session = prepare_mount_session(
            case_conn,
            case_root,
            case_id,
            data_source_id,
            partition_index,
            evidence_mount::MountReadPolicy::default(),
        )
        .map_err(MountRegistryError::Session)?;
        self.mount_session(session, requested_mount_point, &case_id.0)
    }

    pub(crate) fn mount_session(
        &self,
        session: MountSession,
        requested_mount_point: Option<&str>,
        case_id: &str,
    ) -> Result<MountStatusDto, MountRegistryError> {
        let plan = session.plan();
        let mount_id = plan.mount_id.as_str().to_string();
        let status = MountStatusDto {
            target: MountTargetDto {
                mount_id: mount_id.clone(),
                data_source_id: plan.data_source_id.0.clone(),
                partition_index: plan.partition_index as u32,
                filesystem: plan.filesystem_kind.clone(),
                mount_point: requested_mount_point.unwrap_or("").to_string(),
                read_only: true,
                mode: MountModeDto::LogicalPartition,
                physical_device_path: None,
                target_address: None,
            },
            state: MountStateDto::Preparing,
            active_handle_count: 0,
            error: None,
        };
        {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| MountRegistryError::LockPoisoned)?;
            if entries.values().any(|entry| {
                entry.status.target.data_source_id == plan.data_source_id.0
                    && entry.status.target.partition_index == plan.partition_index as u32
            }) {
                return Err(MountRegistryError::AlreadyMounted);
            }
            entries.insert(
                mount_id.clone(),
                MountEntry {
                    case_id: case_id.to_string(),
                    session: session.clone(),
                    status,
                    backend: None,
                },
            );
        }

        let backend = mount_backend::start(session, requested_mount_point);
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| MountRegistryError::LockPoisoned)?;
        let Some(entry) = entries.get_mut(&mount_id) else {
            return Err(MountRegistryError::NotFound(mount_id));
        };
        match backend {
            Ok(backend) => {
                #[cfg(windows)]
                {
                    entry.status.target.mount_point = backend.mount_point();
                }
                entry.status.state = MountStateDto::Mounted;
                entry.backend = Some(backend);
                Ok(entry.status.clone())
            }
            Err(error) => {
                entries.remove(&mount_id);
                Err(MountRegistryError::Backend(error))
            }
        }
    }

    pub(crate) fn unmount(&self, mount_id: &str) -> Result<(), MountRegistryError> {
        let mut entry = self
            .entries
            .lock()
            .map_err(|_| MountRegistryError::LockPoisoned)?
            .remove(mount_id)
            .ok_or_else(|| MountRegistryError::NotFound(mount_id.to_string()))?;
        entry.status.state = MountStateDto::Unmounting;
        if let Some(backend) = entry.backend.as_ref() {
            if let Err(error) = backend.stop() {
                entry.status.state = MountStateDto::Failed;
                entry.status.error = Some(error.to_string());
                self.entries
                    .lock()
                    .map_err(|_| MountRegistryError::LockPoisoned)?
                    .insert(mount_id.to_string(), entry);
                return Err(error.into());
            }
        }
        Ok(())
    }

    pub(crate) fn list(&self) -> Result<Vec<MountStatusDto>, MountRegistryError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| MountRegistryError::LockPoisoned)?;
        for entry in entries.values_mut() {
            refresh_backend_state(entry);
        }
        let mut result = entries.values().map(status_for_entry).collect::<Vec<_>>();
        result.sort_by(|left, right| left.target.mount_id.cmp(&right.target.mount_id));
        Ok(result)
    }

    pub(crate) fn status(&self, mount_id: &str) -> Result<MountStatusDto, MountRegistryError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| MountRegistryError::LockPoisoned)?;
        entries
            .get_mut(mount_id)
            .map(|entry| {
                refresh_backend_state(entry);
                status_for_entry(entry)
            })
            .ok_or_else(|| MountRegistryError::NotFound(mount_id.to_string()))
    }

    pub(crate) fn cleanup_case(&self, case_id: &str) -> Result<(), MountRegistryError> {
        let ids = {
            let entries = self
                .entries
                .lock()
                .map_err(|_| MountRegistryError::LockPoisoned)?;
            entries
                .values()
                .filter(|entry| entry.case_id == case_id)
                .map(|entry| entry.status.target.mount_id.clone())
                .collect::<Vec<_>>()
        };
        for mount_id in ids {
            self.unmount(&mount_id)?;
        }
        Ok(())
    }

    pub(crate) fn cleanup_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), MountRegistryError> {
        let ids = {
            let entries = self
                .entries
                .lock()
                .map_err(|_| MountRegistryError::LockPoisoned)?;
            entries
                .values()
                .filter(|entry| {
                    entry.case_id == case_id && entry.status.target.data_source_id == data_source_id
                })
                .map(|entry| entry.status.target.mount_id.clone())
                .collect::<Vec<_>>()
        };
        for mount_id in ids {
            self.unmount(&mount_id)?;
        }
        Ok(())
    }

    pub(crate) fn cleanup_all(&self) -> Result<(), MountRegistryError> {
        let ids = self
            .entries
            .lock()
            .map_err(|_| MountRegistryError::LockPoisoned)?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for mount_id in ids {
            self.unmount(&mount_id)?;
        }
        Ok(())
    }
}

fn status_for_entry(entry: &MountEntry) -> MountStatusDto {
    let mut status = entry.status.clone();
    status.active_handle_count = entry.session.active_handle_count().unwrap_or(0) as u64;
    status
}

fn refresh_backend_state(entry: &mut MountEntry) {
    if !matches!(entry.status.state, MountStateDto::Mounted) {
        return;
    }
    let Some(backend) = entry.backend.as_ref() else {
        entry.status.state = MountStateDto::Failed;
        entry.status.error = Some("logical mount backend handle is missing".to_string());
        return;
    };
    match backend.poll_exit() {
        Ok(Some(error)) => {
            entry.status.state = MountStateDto::Failed;
            entry.status.error = Some(error);
        }
        Ok(None) => {}
        Err(error) => {
            entry.status.state = MountStateDto::Failed;
            entry.status.error = Some(error.to_string());
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/mount_registry.rs"]
mod tests;
