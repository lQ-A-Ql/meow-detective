use std::net::TcpListener;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use evidence_block::ReadOnlyScsiDevice;
use iscsi_target::{AuthConfig, ChapCredentials, IscsiTarget};
use rand::distributions::{Alphanumeric, DistString};

use crate::PhysicalMountError;

const TARGET_START_TIMEOUT: Duration = Duration::from_secs(3);
const CHAP_SECRET_LENGTH: usize = 16;

pub(crate) struct TargetConnection {
    pub address: String,
    pub port: u16,
    pub iqn: String,
    pub chap_username: String,
    pub chap_secret: String,
}

pub(crate) struct LocalIscsiTarget {
    target: Arc<IscsiTarget<ReadOnlyScsiDevice>>,
    thread: Option<JoinHandle<Result<(), iscsi_target::IscsiError>>>,
    connection: TargetConnection,
}

impl LocalIscsiTarget {
    pub(crate) fn start(
        mount_id: &str,
        device: ReadOnlyScsiDevice,
    ) -> Result<Self, PhysicalMountError> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(iscsi_target::IscsiError::Io)?;
        let local_address = listener
            .local_addr()
            .map_err(iscsi_target::IscsiError::Io)?;
        let iqn_suffix = mount_id.replace('_', "-").to_ascii_lowercase();
        let iqn = format!("iqn.2026-08.local.meow-detective:{iqn_suffix}");
        let alias = format!("Meow~Detective {mount_id}");
        let chap_username = format!("meow-{}", &iqn_suffix[..iqn_suffix.len().min(12)]);
        let chap_secret = Alphanumeric.sample_string(&mut rand::thread_rng(), CHAP_SECRET_LENGTH);
        let auth = AuthConfig::Chap {
            credentials: ChapCredentials::new(&chap_username, &chap_secret),
        };
        let target = Arc::new(
            IscsiTarget::builder()
                .bind_addr(&local_address.to_string())
                .target_name(&iqn)
                .target_alias(&alias)
                .with_auth(auth)
                .max_connections(2)
                .max_sessions(1)
                .build(device)?,
        );
        let server = Arc::clone(&target);
        let thread = std::thread::Builder::new()
            .name(format!("physical-mount-{mount_id}"))
            .spawn(move || server.run_with_listener(listener))
            .map_err(iscsi_target::IscsiError::Io)?;
        let connection = TargetConnection {
            address: local_address.ip().to_string(),
            port: local_address.port(),
            iqn,
            chap_username,
            chap_secret,
        };
        let mut target = Self {
            target,
            thread: Some(thread),
            connection,
        };
        target.wait_until_running()?;
        Ok(target)
    }

    pub(crate) fn connection(&self) -> &TargetConnection {
        &self.connection
    }

    pub(crate) fn stop(&mut self) -> Result<(), PhysicalMountError> {
        self.target.stop();
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| PhysicalMountError::TargetThreadPanicked)??;
        Ok(())
    }

    fn wait_until_running(&mut self) -> Result<(), PhysicalMountError> {
        let deadline = Instant::now() + TARGET_START_TIMEOUT;
        while Instant::now() < deadline {
            if self.target.is_running() {
                return Ok(());
            }
            if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
                let thread = self.thread.take().expect("checked thread handle");
                thread
                    .join()
                    .map_err(|_| PhysicalMountError::TargetThreadPanicked)??;
                return Err(PhysicalMountError::TargetStartupTimeout);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.stop();
        Err(PhysicalMountError::TargetStartupTimeout)
    }
}

impl Drop for LocalIscsiTarget {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
