use std::sync::{mpsc, Mutex, Once};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use dokan::{
    CreateFileInfo, DiskSpaceInfo, FileInfo, FileSystemHandler, FileSystemMounter, FindData,
    MountFlags, MountOptions, OperationInfo, OperationResult, VolumeInfo,
};
use dokan_sys::win32::{FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN};
use evidence_mount::{MountAccess, MountSession};
use widestring::{U16CStr, U16CString};
use winapi::shared::ntstatus::*;
use winapi::um::winnt;

use super::MountBackendError;
use support::{
    file_attributes, has_write_access, map_mount_error, path_from_dokan, stable_file_index,
};

mod support;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
static DOKAN_INIT: Once = Once::new();

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
    let mount_point = choose_mount_point(requested_mount_point)?;
    let mount_point_for_thread = mount_point.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let join_handle = thread::Builder::new()
        .name("meow-detective-dokan".to_string())
        .spawn(move || run_mount_thread(session, mount_point_for_thread, ready_tx))
        .map_err(|error| MountBackendError::Backend(error.to_string()))?;

    match ready_rx.recv_timeout(STARTUP_TIMEOUT) {
        Ok(Ok(())) => Ok(DokanMount {
            mount_point,
            join_handle: Mutex::new(Some(join_handle)),
        }),
        Ok(Err(error)) => {
            let _ = join_handle.join();
            Err(MountBackendError::Backend(error))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = dokan::unmount(&mount_point);
            let _ = join_handle.join();
            Err(MountBackendError::StartupTimeout)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let error = join_handle
                .join()
                .map_err(|_| MountBackendError::Backend("mount thread panicked".to_string()))?
                .err()
                .unwrap_or_else(|| "mount thread exited before startup".to_string());
            Err(MountBackendError::Backend(error))
        }
    }
}

fn run_mount_thread(
    session: MountSession,
    mount_point: U16CString,
    ready_tx: mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let handler = ReadOnlyHandler { session };
    let options = MountOptions {
        flags: MountFlags::WRITE_PROTECT | MountFlags::CURRENT_SESSION,
        timeout: STARTUP_TIMEOUT,
        allocation_unit_size: 4096,
        sector_size: 512,
        ..MountOptions::default()
    };
    let mut mounter = FileSystemMounter::new(&handler, &mount_point, &options);
    let mount_result = mounter.mount();
    match mount_result {
        Ok(file_system) => {
            let _ = ready_tx.send(Ok(()));
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
            let _ = ready_tx.send(Err(message.clone()));
            Err(format!(
                "Dokan mount failed for {}: {message}",
                mount_point.to_string_lossy()
            ))
        }
    }
}

fn choose_mount_point(requested: Option<&str>) -> Result<U16CString, MountBackendError> {
    let point = match requested {
        Some(value) => validate_drive_letter(value)?,
        None => find_free_drive_letter()?,
    };
    U16CString::from_str(point)
        .map_err(|error| MountBackendError::InvalidMountPoint(error.to_string()))
}

fn validate_drive_letter(value: &str) -> Result<String, MountBackendError> {
    let letter = parse_drive_letter(value)?;
    ensure_drive_letter_available(letter)?;
    Ok(dokan_drive_mount_point(letter))
}

fn parse_drive_letter(value: &str) -> Result<u8, MountBackendError> {
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

fn dokan_drive_mount_point(letter: u8) -> String {
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
#[path = "../../tests/unit/mount_backend/dokan.rs"]
mod tests;

struct ReadOnlyHandler {
    session: MountSession,
}

struct MountFileContext {
    session: MountSession,
    handle_id: Option<u64>,
}

impl Drop for MountFileContext {
    fn drop(&mut self) {
        if let Some(handle_id) = self.handle_id.take() {
            let _ = self.session.close(handle_id);
        }
    }
}

impl<'c, 'h: 'c> FileSystemHandler<'c, 'h> for ReadOnlyHandler {
    type Context = MountFileContext;

    fn create_file(
        &'h self,
        file_name: &U16CStr,
        _security_context: &dokan::IO_SECURITY_CONTEXT,
        desired_access: winnt::ACCESS_MASK,
        _file_attributes: u32,
        _share_access: u32,
        create_disposition: u32,
        create_options: u32,
        _info: &mut OperationInfo<'c, 'h, Self>,
    ) -> OperationResult<CreateFileInfo<Self::Context>> {
        let path = path_from_dokan(file_name)?;
        if has_write_access(desired_access)
            || !matches!(
                create_disposition,
                FILE_OPEN | dokan_sys::win32::FILE_OPEN_IF
            )
        {
            return Err(STATUS_ACCESS_DENIED);
        }
        let node = self.session.lookup(&path).map_err(map_mount_error)?;
        let directory_requested = create_options & FILE_DIRECTORY_FILE != 0;
        let file_requested = create_options & FILE_NON_DIRECTORY_FILE != 0;
        if directory_requested && !node.is_dir {
            return Err(STATUS_NOT_A_DIRECTORY);
        }
        if file_requested && node.is_dir {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        let handle_id = (!node.is_dir)
            .then(|| self.session.open(&path, MountAccess::ReadOnly))
            .transpose()
            .map_err(map_mount_error)?;
        Ok(CreateFileInfo {
            context: MountFileContext {
                session: self.session.clone(),
                handle_id,
            },
            is_dir: node.is_dir,
            new_file_created: false,
        })
    }

    fn read_file(
        &'h self,
        _file_name: &U16CStr,
        offset: i64,
        buffer: &mut [u8],
        _info: &OperationInfo<'c, 'h, Self>,
        context: &'c Self::Context,
    ) -> OperationResult<u32> {
        if offset < 0 {
            return Err(STATUS_INVALID_PARAMETER);
        }
        let handle_id = context.handle_id.ok_or(STATUS_FILE_IS_A_DIRECTORY)?;
        let data = context
            .session
            .read_at(handle_id, offset as u64, buffer.len())
            .map_err(map_mount_error)?;
        buffer[..data.len()].copy_from_slice(&data);
        u32::try_from(data.len()).map_err(|_| STATUS_INVALID_BUFFER_SIZE)
    }

    fn get_file_information(
        &'h self,
        file_name: &U16CStr,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<FileInfo> {
        let path = path_from_dokan(file_name)?;
        let node = self.session.lookup(&path).map_err(map_mount_error)?;
        Ok(FileInfo {
            attributes: file_attributes(&node),
            creation_time: node.created_at.unwrap_or(SystemTime::UNIX_EPOCH),
            last_access_time: node.accessed_at.unwrap_or(SystemTime::UNIX_EPOCH),
            last_write_time: node.modified_at.unwrap_or(SystemTime::UNIX_EPOCH),
            file_size: node.size,
            number_of_links: 1,
            file_index: stable_file_index(path.as_str()),
        })
    }

    fn find_files(
        &'h self,
        file_name: &U16CStr,
        mut fill_find_data: impl FnMut(&FindData) -> dokan::FillDataResult,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        support::find_directory_files(&self.session, file_name, &mut fill_find_data)
    }

    fn flush_file_buffers(
        &'h self,
        _file_name: &U16CStr,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Ok(())
    }

    fn write_file(
        &'h self,
        _file_name: &U16CStr,
        _offset: i64,
        _buffer: &[u8],
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<u32> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn set_file_attributes(
        &'h self,
        _file_name: &U16CStr,
        _file_attributes: u32,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn set_file_time(
        &'h self,
        _file_name: &U16CStr,
        _creation_time: dokan::FileTimeOperation,
        _last_access_time: dokan::FileTimeOperation,
        _last_write_time: dokan::FileTimeOperation,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn delete_file(
        &'h self,
        _file_name: &U16CStr,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn delete_directory(
        &'h self,
        _file_name: &U16CStr,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn move_file(
        &'h self,
        _file_name: &U16CStr,
        _new_file_name: &U16CStr,
        _replace_if_existing: bool,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn set_end_of_file(
        &'h self,
        _file_name: &U16CStr,
        _offset: i64,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn set_allocation_size(
        &'h self,
        _file_name: &U16CStr,
        _alloc_size: i64,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn set_file_security(
        &'h self,
        _file_name: &U16CStr,
        _security_information: u32,
        _security_descriptor: winnt::PSECURITY_DESCRIPTOR,
        _buffer_length: u32,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        Err(STATUS_ACCESS_DENIED)
    }

    fn get_volume_information(
        &'h self,
        _info: &OperationInfo<'c, 'h, Self>,
    ) -> OperationResult<VolumeInfo> {
        Ok(VolumeInfo {
            name: U16CString::from_str("Meow Detective Evidence")
                .map_err(|_| STATUS_OBJECT_NAME_INVALID)?,
            serial_number: 0,
            max_component_length: 255,
            fs_flags: winnt::FILE_CASE_PRESERVED_NAMES
                | winnt::FILE_UNICODE_ON_DISK
                | winnt::FILE_READ_ONLY_VOLUME,
            fs_name: support::windows_filesystem_name(&self.session.plan().filesystem_kind)?,
        })
    }

    fn get_disk_free_space(
        &'h self,
        _info: &OperationInfo<'c, 'h, Self>,
    ) -> OperationResult<DiskSpaceInfo> {
        let size = self.session.plan().volume_size;
        Ok(DiskSpaceInfo {
            byte_count: size,
            free_byte_count: 0,
            available_byte_count: 0,
        })
    }

    fn mounted(
        &'h self,
        _mount_point: &U16CStr,
        _info: &OperationInfo<'c, 'h, Self>,
    ) -> OperationResult<()> {
        Ok(())
    }

    fn unmounted(&'h self, _info: &OperationInfo<'c, 'h, Self>) -> OperationResult<()> {
        Ok(())
    }
}
