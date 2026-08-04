use std::time::SystemTime;

use dokan::{
    CreateFileInfo, DiskSpaceInfo, FileInfo, FileSystemHandler, FindData, OperationInfo,
    OperationResult, VolumeInfo,
};
use dokan_sys::win32::{FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN};
use evidence_mount::{MountAccess, MountSession};
use widestring::{U16CStr, U16CString};
use winapi::shared::ntstatus::*;
use winapi::um::winnt;

use support::{
    file_attributes, has_write_access, map_mount_error, path_from_dokan, stable_file_index,
};

mod lifecycle;
mod support;

pub(crate) use lifecycle::{start, DokanMount};

#[cfg(test)]
#[path = "../../tests/unit/mount_backend/dokan.rs"]
mod tests;

struct ReadOnlyHandler {
    session: MountSession,
    publication: lifecycle::MountPublication,
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
        mount_point: &U16CStr,
        _info: &OperationInfo<'c, 'h, Self>,
    ) -> OperationResult<()> {
        self.publication.publish(mount_point);
        Ok(())
    }

    fn unmounted(&'h self, _info: &OperationInfo<'c, 'h, Self>) -> OperationResult<()> {
        Ok(())
    }
}
