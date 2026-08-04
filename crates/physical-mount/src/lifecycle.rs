use std::path::Path;

use evidence_block::{EvidenceImageKind, ReadOnlyScsiDevice};

use crate::target::LocalIscsiTarget;
use crate::PhysicalMountError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalImageKind {
    E01,
    Raw,
}

impl From<PhysicalImageKind> for EvidenceImageKind {
    fn from(value: PhysicalImageKind) -> Self {
        match value {
            PhysicalImageKind::E01 => Self::E01,
            PhysicalImageKind::Raw => Self::Raw,
        }
    }
}

pub struct PhysicalMount {
    mount_id: String,
    target: LocalIscsiTarget,
    #[cfg(windows)]
    session: Option<crate::windows_initiator::WindowsIscsiSession>,
    #[cfg(windows)]
    service_lease: Option<crate::windows_service::IscsiServiceLease>,
}

impl PhysicalMount {
    #[cfg(windows)]
    pub fn start(path: &Path, kind: PhysicalImageKind) -> Result<Self, PhysicalMountError> {
        let mount_id = format!("physical-{}", uuid::Uuid::new_v4().simple());
        let device = ReadOnlyScsiDevice::open(path, kind.into())?;
        let mut target = LocalIscsiTarget::start(&mount_id, device)?;
        let service_lease = match crate::windows_service::IscsiServiceLease::acquire() {
            Ok(lease) => lease,
            Err(error) => {
                let _ = target.stop();
                return Err(error);
            }
        };
        let session =
            match crate::windows_initiator::WindowsIscsiSession::connect(target.connection()) {
                Ok(session) => session,
                Err(error) => {
                    let _ = target.stop();
                    drop(service_lease);
                    return Err(error);
                }
            };
        Ok(Self {
            mount_id,
            target,
            session: Some(session),
            service_lease: Some(service_lease),
        })
    }

    #[cfg(not(windows))]
    pub fn start(_path: &Path, _kind: PhysicalImageKind) -> Result<Self, PhysicalMountError> {
        Err(PhysicalMountError::UnsupportedPlatform)
    }

    pub fn mount_id(&self) -> &str {
        &self.mount_id
    }

    #[cfg(windows)]
    pub fn physical_device_path(&self) -> Option<&str> {
        self.session
            .as_ref()
            .and_then(|session| session.primary_device_path())
    }

    #[cfg(not(windows))]
    pub fn physical_device_path(&self) -> Option<&str> {
        None
    }

    pub fn target_address(&self) -> String {
        let connection = self.target.connection();
        format!("{}:{}", connection.address, connection.port)
    }

    pub fn target_iqn(&self) -> &str {
        &self.target.connection().iqn
    }

    pub fn stop(&mut self) -> Result<(), PhysicalMountError> {
        #[cfg(windows)]
        let session_result = self
            .session
            .take()
            .map(|mut session| session.disconnect())
            .unwrap_or(Ok(()));
        #[cfg(not(windows))]
        let session_result = Ok(());
        let target_result = self.target.stop();
        #[cfg(windows)]
        let service_result = self
            .service_lease
            .take()
            .map(|mut lease| lease.release())
            .unwrap_or(Ok(()));
        #[cfg(not(windows))]
        let service_result = Ok(());
        session_result.and(target_result).and(service_result)
    }
}

impl Drop for PhysicalMount {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
