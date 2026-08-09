use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dokan::{FileSystemMounter, MountFlags, MountOptions};
use evidence_emulation::CowDisk;
use widestring::{U16CStr, U16CString};
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};

use crate::emulation_backend::EmulationBackendError;

use super::ExtentHandler;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) enum StartupEvent {
    Mounted(U16CString),
    Failed(String),
}

pub(super) struct MountPublication {
    sender: Mutex<Option<mpsc::SyncSender<StartupEvent>>>,
}

impl MountPublication {
    pub(super) fn new(sender: mpsc::SyncSender<StartupEvent>) -> Self {
        Self {
            sender: Mutex::new(Some(sender)),
        }
    }

    pub(super) fn publish(&self, mount_point: &U16CStr) {
        let Ok(mut sender) = self.sender.lock() else {
            tracing::error!("emulation Dokan publication lock is poisoned");
            return;
        };
        if let Some(sender) = sender.take() {
            let _ = sender.send(StartupEvent::Mounted(mount_point.to_owned()));
        }
    }
}

pub(crate) struct DokanExtentMount {
    mount_point: U16CString,
    join_handle: Mutex<Option<JoinHandle<Result<(), String>>>>,
}

impl DokanExtentMount {
    pub(crate) fn poll_exit(&self) -> Result<Option<String>, EmulationBackendError> {
        let mut handle = self
            .join_handle
            .lock()
            .map_err(|_| backend_error("mount thread lock is poisoned"))?;
        let Some(worker) = handle.as_ref() else {
            return Ok(Some("Dokan extent worker is no longer active".to_string()));
        };
        if !worker.is_finished() {
            return Ok(None);
        }
        let worker = handle
            .take()
            .ok_or_else(|| backend_error("mount worker state changed"))?;
        Ok(Some(match worker.join() {
            Ok(Ok(())) => "Dokan extent worker exited unexpectedly".to_string(),
            Ok(Err(error)) => error,
            Err(_) => "Dokan extent worker panicked".to_string(),
        }))
    }

    pub(crate) fn stop(&self) -> Result<(), EmulationBackendError> {
        let handle = self
            .join_handle
            .lock()
            .map_err(|_| backend_error("mount thread lock is poisoned"))?
            .take();
        if handle.is_none() {
            return Ok(());
        }
        if !dokan::unmount(&self.mount_point) {
            if let Ok(mut guard) = self.join_handle.lock() {
                *guard = handle;
            }
            return Err(backend_error("Dokan rejected the extent unmount request"));
        }
        if let Some(handle) = handle {
            handle
                .join()
                .map_err(|_| backend_error("extent mount thread panicked"))?
                .map_err(EmulationBackendError::Backend)?;
        }
        Ok(())
    }
}

impl Drop for DokanExtentMount {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub(crate) fn start(
    disk: Arc<CowDisk>,
    session_root: &Path,
    mount_point: &Path,
) -> Result<DokanExtentMount, EmulationBackendError> {
    crate::dokan_runtime::initialize();
    if dokan::get_driver_version() == 0 {
        return Err(EmulationBackendError::Backend(
            "the Dokan 2.x driver is not installed on this machine; emulation mounts require it"
                .to_string(),
        ));
    }
    let mount_point = validate_mount_directory(session_root, mount_point)?;
    let encoded = path_to_u16(&mount_point)?;
    let mount_point_for_thread = encoded.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let join_handle = thread::Builder::new()
        .name("meow-detective-emulation-dokan".to_string())
        .spawn(move || run_mount_thread(disk, mount_point_for_thread, ready_tx))
        .map_err(|error| EmulationBackendError::Backend(error.to_string()))?;
    finish_start(encoded, join_handle, ready_rx)
}

fn finish_start(
    requested: U16CString,
    join_handle: JoinHandle<Result<(), String>>,
    ready_rx: mpsc::Receiver<StartupEvent>,
) -> Result<DokanExtentMount, EmulationBackendError> {
    match ready_rx.recv_timeout(STARTUP_TIMEOUT) {
        Ok(StartupEvent::Mounted(actual)) => Ok(DokanExtentMount {
            mount_point: actual,
            join_handle: Mutex::new(Some(join_handle)),
        }),
        Ok(StartupEvent::Failed(error)) => {
            let _ = join_handle.join();
            Err(EmulationBackendError::Backend(error))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            if dokan::unmount(&requested) {
                // The worker should now exit on its own; join to reap it.
                let _ = join_handle.join();
            } else {
                // Without a successful unmount the worker may stay blocked
                // inside Dokan; joining here could hang prepare forever, so
                // detach the thread instead of reaping it.
                tracing::warn!(
                    "Dokan unmount after startup timeout failed; detaching the extent mount worker"
                );
                drop(join_handle);
            }
            Err(EmulationBackendError::StartupTimeout)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            if !dokan::unmount(&requested) {
                tracing::warn!(
                    "Dokan unmount after worker disconnect failed; detaching the extent mount worker"
                );
                drop(join_handle);
                return Err(EmulationBackendError::Backend(
                    "extent worker exited before publication".to_string(),
                ));
            }
            let error = join_handle
                .join()
                .map_err(|_| backend_error("extent mount thread panicked"))?
                .err()
                .unwrap_or_else(|| "extent worker exited before publication".to_string());
            Err(EmulationBackendError::Backend(error))
        }
    }
}

fn run_mount_thread(
    disk: Arc<CowDisk>,
    mount_point: U16CString,
    ready_tx: mpsc::SyncSender<StartupEvent>,
) -> Result<(), String> {
    let handler = ExtentHandler {
        disk,
        publication: MountPublication::new(ready_tx.clone()),
    };
    let options = MountOptions {
        flags: mount_flags(),
        timeout: STARTUP_TIMEOUT,
        allocation_unit_size: 4096,
        sector_size: 512,
        ..MountOptions::default()
    };
    let mut mounter = FileSystemMounter::new(&handler, &mount_point, &options);
    let result = match mounter.mount() {
        Ok(file_system) => {
            drop(file_system);
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            tracing::error!(error = %message, "Dokan emulation extent mount failed");
            let _ = ready_tx.send(StartupEvent::Failed(message.clone()));
            Err(format!("Dokan emulation extent mount failed: {message}"))
        }
    };
    result
}

pub(super) fn mount_flags() -> MountFlags {
    MountFlags::CURRENT_SESSION
}

fn validate_mount_directory(
    session_root: &Path,
    mount_point: &Path,
) -> Result<PathBuf, EmulationBackendError> {
    let root = session_root.canonicalize().map_err(invalid_mount_io)?;
    let point = mount_point.canonicalize().map_err(invalid_mount_io)?;
    if point.parent() != Some(root.as_path())
        || point.file_name().and_then(|v| v.to_str()) != Some("mount")
    {
        return Err(invalid_mount(
            "mount point must be the session-owned mount directory",
        ));
    }
    let metadata = std::fs::symlink_metadata(&point).map_err(invalid_mount_io)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err(invalid_mount("mount point must be a regular directory"));
    }
    if std::fs::read_dir(&point)
        .map_err(invalid_mount_io)?
        .next()
        .is_some()
    {
        return Err(invalid_mount("mount point must be empty"));
    }
    ensure_ntfs(&point)?;
    Ok(point)
}

fn ensure_ntfs(path: &Path) -> Result<(), EmulationBackendError> {
    let path = path_to_u16(path)?;
    let mut volume_path = [0u16; 261];
    // SAFETY: Both buffers are valid NUL-terminated UTF-16 storage for the duration of the calls.
    unsafe { GetVolumePathNameW(PCWSTR(path.as_ptr()), &mut volume_path) }
        .map_err(|error| invalid_mount(error.to_string()))?;
    let mut filesystem_name = [0u16; 32];
    // SAFETY: The volume path returned above is NUL-terminated and all optional output buffers are valid.
    unsafe {
        GetVolumeInformationW(
            PCWSTR(volume_path.as_ptr()),
            None,
            None,
            None,
            None,
            Some(&mut filesystem_name),
        )
    }
    .map_err(|error| invalid_mount(error.to_string()))?;
    let end = filesystem_name
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(filesystem_name.len());
    let filesystem = String::from_utf16_lossy(&filesystem_name[..end]);
    if !filesystem.eq_ignore_ascii_case("NTFS") {
        return Err(invalid_mount("mount point must reside on an NTFS volume"));
    }
    Ok(())
}

fn path_to_u16(path: &Path) -> Result<U16CString, EmulationBackendError> {
    U16CString::from_os_str(path.as_os_str())
        .map_err(|error| invalid_mount(format!("path encoding failed: {error}")))
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & winapi::um::winnt::FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn invalid_mount_io(error: std::io::Error) -> EmulationBackendError {
    invalid_mount(error.to_string())
}

fn invalid_mount(message: impl Into<String>) -> EmulationBackendError {
    EmulationBackendError::InvalidMountPoint(message.into())
}

fn backend_error(message: impl Into<String>) -> EmulationBackendError {
    EmulationBackendError::Backend(message.into())
}

#[cfg(test)]
#[path = "../../../tests/unit/emulation_backend/dokan_lifecycle.rs"]
mod tests;
