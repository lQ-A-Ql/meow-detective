use std::sync::{mpsc, Mutex, Once};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dokan::{FileSystemMounter, MountFlags, MountOptions};
use evidence_mount::MountSession;
use widestring::{U16CStr, U16CString};

use crate::mount_backend::MountBackendError;

use super::ReadOnlyHandler;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
static DOKAN_INIT: Once = Once::new();

enum StartupEvent {
    Mounted(U16CString),
    Failed(String),
}

pub(super) struct MountPublication {
    sender: Mutex<Option<mpsc::SyncSender<StartupEvent>>>,
}

impl MountPublication {
    fn new(sender: mpsc::SyncSender<StartupEvent>) -> Self {
        Self {
            sender: Mutex::new(Some(sender)),
        }
    }

    pub(super) fn publish(&self, mount_point: &U16CStr) {
        let Ok(mut sender) = self.sender.lock() else {
            tracing::error!("Dokan mount publication lock is poisoned");
            return;
        };
        if let Some(sender) = sender.take() {
            let _ = sender.send(StartupEvent::Mounted(mount_point.to_owned()));
        }
    }
}

pub(crate) struct DokanMount {
    mount_point: U16CString,
    join_handle: Mutex<Option<JoinHandle<Result<(), String>>>>,
}

impl DokanMount {
    pub(crate) fn mount_point(&self) -> String {
        self.mount_point
            .to_string_lossy()
            .trim_end_matches('\\')
            .to_string()
    }

    pub(crate) fn poll_exit(&self) -> Result<Option<String>, MountBackendError> {
        let mut handle = self
            .join_handle
            .lock()
            .map_err(|_| MountBackendError::Backend("mount thread lock is poisoned".to_string()))?;
        let Some(worker) = handle.as_ref() else {
            return Ok(Some("Dokan mount worker is no longer active".to_string()));
        };
        if !worker.is_finished() {
            return Ok(None);
        }
        let worker = handle
            .take()
            .ok_or_else(|| MountBackendError::Backend("mount worker state changed".to_string()))?;
        let message = match worker.join() {
            Ok(Ok(())) => "Dokan mount worker exited unexpectedly".to_string(),
            Ok(Err(error)) => error,
            Err(_) => "Dokan mount worker panicked".to_string(),
        };
        Ok(Some(message))
    }

    pub(crate) fn stop(&self) -> Result<(), MountBackendError> {
        let handle = self
            .join_handle
            .lock()
            .map_err(|_| MountBackendError::Backend("mount thread lock is poisoned".to_string()))?
            .take();
        if handle.is_none() {
            return Ok(());
        }
        if !dokan::unmount(&self.mount_point) {
            if let Ok(mut guard) = self.join_handle.lock() {
                *guard = handle;
            }
            return Err(MountBackendError::Backend(
                "Dokan rejected the unmount request".to_string(),
            ));
        }
        if let Some(handle) = handle {
            handle
                .join()
                .map_err(|_| MountBackendError::Backend("mount thread panicked".to_string()))?
                .map_err(MountBackendError::Backend)?;
        }
        Ok(())
    }
}

impl Drop for DokanMount {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub(crate) fn start(
    session: MountSession,
    requested_mount_point: Option<&str>,
) -> Result<DokanMount, MountBackendError> {
    DOKAN_INIT.call_once(dokan::init);
    let requested_mount_point = choose_mount_point(requested_mount_point)?;
    let mount_point_for_thread = requested_mount_point.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let join_handle = thread::Builder::new()
        .name("meow-detective-dokan".to_string())
        .spawn(move || run_mount_thread(session, mount_point_for_thread, ready_tx))
        .map_err(|error| MountBackendError::Backend(error.to_string()))?;

    match ready_rx.recv_timeout(STARTUP_TIMEOUT) {
        Ok(StartupEvent::Mounted(actual_mount_point)) => Ok(DokanMount {
            mount_point: actual_mount_point,
            join_handle: Mutex::new(Some(join_handle)),
        }),
        Ok(StartupEvent::Failed(error)) => {
            let _ = join_handle.join();
            Err(MountBackendError::Backend(error))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = dokan::unmount(&requested_mount_point);
            let _ = join_handle.join();
            Err(MountBackendError::StartupTimeout)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let error = join_handle
                .join()
                .map_err(|_| MountBackendError::Backend("mount thread panicked".to_string()))?
                .err()
                .unwrap_or_else(|| "mount thread exited before publication".to_string());
            Err(MountBackendError::Backend(error))
        }
    }
}

fn run_mount_thread(
    session: MountSession,
    mount_point: U16CString,
    ready_tx: mpsc::SyncSender<StartupEvent>,
) -> Result<(), String> {
    let handler = ReadOnlyHandler {
        session,
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
    let mount_result = mounter.mount();
    match mount_result {
        Ok(file_system) => {
            drop(file_system);
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            tracing::error!(
                mount_point = %mount_point.to_string_lossy(),
                error = %message,
                "Dokan mount failed"
            );
            let _ = ready_tx.send(StartupEvent::Failed(message.clone()));
            Err(format!(
                "Dokan mount failed for {}: {message}",
                mount_point.to_string_lossy()
            ))
        }
    }
}

pub(super) fn mount_flags() -> MountFlags {
    MountFlags::WRITE_PROTECT | MountFlags::MOUNT_MANAGER
}

fn choose_mount_point(requested: Option<&str>) -> Result<U16CString, MountBackendError> {
    let point = match requested {
        Some(value) => validate_drive_letter(value)?,
        None => find_free_drive_letter()?,
    };
    U16CString::from_str(point)
        .map_err(|error| MountBackendError::InvalidMountPoint(error.to_string()))
}

pub(super) fn validate_drive_letter(value: &str) -> Result<String, MountBackendError> {
    let letter = parse_drive_letter(value)?;
    ensure_drive_letter_available(letter)?;
    Ok(dokan_drive_mount_point(letter))
}

pub(super) fn parse_drive_letter(value: &str) -> Result<u8, MountBackendError> {
    let value = value.trim().trim_end_matches('\\');
    let bytes = value.as_bytes();
    if bytes.len() != 2 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return Err(MountBackendError::InvalidMountPoint(
            "v1 accepts a drive letter such as M:".to_string(),
        ));
    }
    Ok(bytes[0].to_ascii_uppercase())
}

fn ensure_drive_letter_available(letter: u8) -> Result<(), MountBackendError> {
    // SAFETY: GetLogicalDrives is a read-only process-wide query with no pointer arguments.
    let drives = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
    let index = u32::from(letter - b'A');
    if drives & (1u32 << index) != 0 || dokan_drive_is_mounted(letter) {
        return Err(MountBackendError::InvalidMountPoint(format!(
            "drive {letter}: is already in use"
        )));
    }
    Ok(())
}

fn find_free_drive_letter() -> Result<String, MountBackendError> {
    // SAFETY: GetLogicalDrives is a read-only process-wide query with no pointer arguments.
    let drives = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
    for index in 3..26 {
        let letter = b'A' + u8::try_from(index).unwrap_or(25);
        if drives & (1u32 << index) == 0 && !dokan_drive_is_mounted(letter) {
            return Ok(dokan_drive_mount_point(letter));
        }
    }
    Err(MountBackendError::InvalidMountPoint(
        "no free drive letter is available".to_string(),
    ))
}

pub(super) fn dokan_drive_mount_point(letter: u8) -> String {
    format!("{}:\\", char::from(letter))
}

fn dokan_drive_is_mounted(letter: u8) -> bool {
    let suffix = format!("{}:", char::from(letter));
    dokan::list_mount_points(false).is_some_and(|mount_points| {
        mount_points.into_iter().any(|mount_point| {
            mount_point.mount_point.is_some_and(|value| {
                value
                    .to_string_lossy()
                    .to_ascii_uppercase()
                    .ends_with(&suffix)
            })
        })
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/mount_backend/dokan_lifecycle.rs"]
mod tests;
