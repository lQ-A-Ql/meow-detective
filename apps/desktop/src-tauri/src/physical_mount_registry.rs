use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use app_services::mount_service::{
    prepare_physical_mount_source, MountServiceError, PreparedPhysicalImageKind,
};
use physical_mount::{PhysicalImageKind, PhysicalMount, PhysicalMountError};
use thiserror::Error;
use transport::dto::{MountModeDto, MountStateDto, MountStatusDto, MountTargetDto};

#[derive(Clone, Default)]
pub struct PhysicalMountRegistry {
    entries: Arc<Mutex<HashMap<String, PhysicalMountEntry>>>,
}

struct PhysicalMountEntry {
    case_id: String,
    status: MountStatusDto,
    mount: PhysicalMount,
}

#[derive(Debug, Error)]
pub(crate) enum PhysicalMountRegistryError {
    #[error("physical mount registry lock is poisoned")]
    LockPoisoned,
    #[error("physical mount {0} was not found")]
    NotFound(String),
    #[error("the data source is already mounted as a physical disk")]
    AlreadyMounted,
    #[error("physical mount source error: {0}")]
    Source(#[from] MountServiceError),
    #[error("physical mount backend error: {0}")]
    Backend(#[from] PhysicalMountError),
}

impl transport::ServiceErrorCategory for PhysicalMountRegistryError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::LockPoisoned => transport::ErrorCategory::Internal,
            Self::NotFound(_) | Self::AlreadyMounted => transport::ErrorCategory::Validation,
            Self::Source(error) => error.category(),
            Self::Backend(PhysicalMountError::UnsupportedPlatform) => {
                transport::ErrorCategory::Unsupported
            }
            Self::Backend(PhysicalMountError::TargetStartupTimeout) => {
                transport::ErrorCategory::Timeout
            }
            Self::Backend(
                PhysicalMountError::IscsiServiceRequiresElevation
                | PhysicalMountError::IscsiLoginRequiresElevation,
            ) => transport::ErrorCategory::Security,
            Self::Backend(
                PhysicalMountError::IscsiServiceLeaseState
                | PhysicalMountError::IscsiServiceCoordinatorPoisoned,
            ) => transport::ErrorCategory::Internal,
            Self::Backend(PhysicalMountError::BlockDevice(_)) => transport::ErrorCategory::Io,
            Self::Backend(_) => transport::ErrorCategory::External,
        }
    }
}

impl PhysicalMountRegistry {
    pub(crate) fn mount(
        &self,
        case_conn: &rusqlite::Connection,
        case_id: &domain::CaseId,
        data_source_id: &domain::DataSourceId,
    ) -> Result<MountStatusDto, PhysicalMountRegistryError> {
        self.reject_duplicate(data_source_id)?;
        let prepared = prepare_physical_mount_source(case_conn, data_source_id)?;
        let kind = match prepared.image_kind {
            PreparedPhysicalImageKind::E01 => PhysicalImageKind::E01,
            PreparedPhysicalImageKind::Raw => PhysicalImageKind::Raw,
        };
        tracing::info!(
            data_source_id = %data_source_id.0,
            source_binding = %prepared.source_binding,
            "Starting read-only physical-disk mount"
        );
        let mount = PhysicalMount::start(&prepared.source_path, kind)?;
        let mount_id = mount.mount_id().to_string();
        let device_path = mount.physical_device_path().map(str::to_string);
        let status = MountStatusDto {
            target: MountTargetDto {
                mount_id: mount_id.clone(),
                data_source_id: data_source_id.0.clone(),
                partition_index: 0,
                filesystem: "physical-disk".to_string(),
                mount_point: device_path
                    .clone()
                    .unwrap_or_else(|| mount.target_iqn().to_string()),
                read_only: true,
                mode: MountModeDto::PhysicalDisk,
                physical_device_path: device_path,
                target_address: Some(mount.target_address()),
            },
            state: MountStateDto::Mounted,
            active_handle_count: 0,
            error: None,
        };
        self.entries
            .lock()
            .map_err(|_| PhysicalMountRegistryError::LockPoisoned)?
            .insert(
                mount_id,
                PhysicalMountEntry {
                    case_id: case_id.0.clone(),
                    status: status.clone(),
                    mount,
                },
            );
        Ok(status)
    }

    pub(crate) fn unmount(&self, mount_id: &str) -> Result<(), PhysicalMountRegistryError> {
        let mut entry = self
            .entries
            .lock()
            .map_err(|_| PhysicalMountRegistryError::LockPoisoned)?
            .remove(mount_id)
            .ok_or_else(|| PhysicalMountRegistryError::NotFound(mount_id.to_string()))?;
        entry.mount.stop()?;
        Ok(())
    }

    pub(crate) fn status(
        &self,
        mount_id: &str,
    ) -> Result<MountStatusDto, PhysicalMountRegistryError> {
        self.entries
            .lock()
            .map_err(|_| PhysicalMountRegistryError::LockPoisoned)?
            .get(mount_id)
            .map(|entry| entry.status.clone())
            .ok_or_else(|| PhysicalMountRegistryError::NotFound(mount_id.to_string()))
    }

    pub(crate) fn list(&self) -> Result<Vec<MountStatusDto>, PhysicalMountRegistryError> {
        let mut mounts = self
            .entries
            .lock()
            .map_err(|_| PhysicalMountRegistryError::LockPoisoned)?
            .values()
            .map(|entry| entry.status.clone())
            .collect::<Vec<_>>();
        mounts.sort_by(|left, right| left.target.mount_id.cmp(&right.target.mount_id));
        Ok(mounts)
    }

    pub(crate) fn cleanup_case(&self, case_id: &str) -> Result<(), PhysicalMountRegistryError> {
        self.cleanup_matching(|entry| entry.case_id == case_id)
    }

    pub(crate) fn cleanup_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), PhysicalMountRegistryError> {
        self.cleanup_matching(|entry| {
            entry.case_id == case_id && entry.status.target.data_source_id == data_source_id
        })
    }

    fn reject_duplicate(
        &self,
        data_source_id: &domain::DataSourceId,
    ) -> Result<(), PhysicalMountRegistryError> {
        let duplicate = self
            .entries
            .lock()
            .map_err(|_| PhysicalMountRegistryError::LockPoisoned)?
            .values()
            .any(|entry| entry.status.target.data_source_id == data_source_id.0);
        if duplicate {
            return Err(PhysicalMountRegistryError::AlreadyMounted);
        }
        Ok(())
    }

    fn cleanup_matching(
        &self,
        predicate: impl Fn(&PhysicalMountEntry) -> bool,
    ) -> Result<(), PhysicalMountRegistryError> {
        let ids = self
            .entries
            .lock()
            .map_err(|_| PhysicalMountRegistryError::LockPoisoned)?
            .iter()
            .filter(|(_, entry)| predicate(entry))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.unmount(&id)?;
        }
        Ok(())
    }
}

impl Drop for PhysicalMountRegistry {
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
            let _ = self.unmount(&id);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/physical_mount_registry.rs"]
mod tests;
