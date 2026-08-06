use std::sync::Arc;
use std::time::SystemTime;

use dokan::{
    CreateFileInfo, DiskSpaceInfo, FileInfo, FileSystemHandler, FillDataError, FindData,
    OperationInfo, OperationResult, VolumeInfo,
};
use dokan_sys::win32::{FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN};
use evidence_emulation::{CowDisk, EmulationError};
use widestring::{U16CStr, U16CString};
use winapi::shared::{ntdef::NTSTATUS, ntstatus::*};
use winapi::um::winnt;

mod lifecycle;

pub(crate) use lifecycle::{start, DokanExtentMount};

const DISK_NAME: &str = "disk.raw";
const ROOT_FILE_INDEX: u64 = 1;
const DISK_FILE_INDEX: u64 = 2;

struct ExtentHandler {
    disk: Arc<CowDisk>,
    publication: lifecycle::MountPublication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtentNode {
    Root,
    Disk,
}

impl<'c, 'h: 'c> FileSystemHandler<'c, 'h> for ExtentHandler {
    type Context = ExtentNode;

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
        let node = extent_node(file_name)?;
        if create_disposition != FILE_OPEN || forbidden_access(node, desired_access) {
            return Err(STATUS_ACCESS_DENIED);
        }
        let directory_requested = create_options & FILE_DIRECTORY_FILE != 0;
        let file_requested = create_options & FILE_NON_DIRECTORY_FILE != 0;
        if directory_requested && node != ExtentNode::Root {
            return Err(STATUS_NOT_A_DIRECTORY);
        }
        if file_requested && node == ExtentNode::Root {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        Ok(CreateFileInfo {
            context: node,
            is_dir: node == ExtentNode::Root,
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
        require_disk(*context)?;
        let count = bounded_read_length(offset, buffer.len(), self.disk.len())?;
        self.disk
            .read_exact_at(offset as u64, &mut buffer[..count])
            .map_err(map_emulation_error)?;
        u32::try_from(count).map_err(|_| STATUS_INVALID_BUFFER_SIZE)
    }

    fn write_file(
        &'h self,
        _file_name: &U16CStr,
        offset: i64,
        buffer: &[u8],
        info: &OperationInfo<'c, 'h, Self>,
        context: &'c Self::Context,
    ) -> OperationResult<u32> {
        require_disk(*context)?;
        if offset < 0 || info.write_to_eof() {
            return Err(STATUS_INVALID_PARAMETER);
        }
        self.disk
            .write_all_at(offset as u64, buffer)
            .map_err(map_emulation_error)?;
        u32::try_from(buffer.len()).map_err(|_| STATUS_INVALID_BUFFER_SIZE)
    }

    fn flush_file_buffers(
        &'h self,
        _file_name: &U16CStr,
        _info: &OperationInfo<'c, 'h, Self>,
        context: &'c Self::Context,
    ) -> OperationResult<()> {
        require_disk(*context)?;
        self.disk.flush().map_err(map_emulation_error)
    }

    fn get_file_information(
        &'h self,
        file_name: &U16CStr,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<FileInfo> {
        let node = extent_node(file_name)?;
        Ok(file_information(node, self.disk.len()))
    }

    fn find_files(
        &'h self,
        file_name: &U16CStr,
        mut fill_find_data: impl FnMut(&FindData) -> dokan::FillDataResult,
        _info: &OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> OperationResult<()> {
        if extent_node(file_name)? != ExtentNode::Root {
            return Err(STATUS_NOT_A_DIRECTORY);
        }
        let data = FindData {
            attributes: disk_attributes(),
            creation_time: SystemTime::UNIX_EPOCH,
            last_access_time: SystemTime::UNIX_EPOCH,
            last_write_time: SystemTime::UNIX_EPOCH,
            file_size: self.disk.len(),
            file_name: U16CString::from_str(DISK_NAME).map_err(|_| STATUS_OBJECT_NAME_INVALID)?,
        };
        match fill_find_data(&data) {
            Ok(()) | Err(FillDataError::NameTooLong) => Ok(()),
            Err(FillDataError::BufferFull) => Err(STATUS_BUFFER_OVERFLOW),
        }
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
            name: U16CString::from_str("Meow Detective Emulation")
                .map_err(|_| STATUS_OBJECT_NAME_INVALID)?,
            serial_number: 0,
            max_component_length: 255,
            fs_flags: winnt::FILE_CASE_PRESERVED_NAMES | winnt::FILE_UNICODE_ON_DISK,
            fs_name: U16CString::from_str("NTFS").map_err(|_| STATUS_OBJECT_NAME_INVALID)?,
        })
    }

    fn get_disk_free_space(
        &'h self,
        _info: &OperationInfo<'c, 'h, Self>,
    ) -> OperationResult<DiskSpaceInfo> {
        Ok(DiskSpaceInfo {
            byte_count: self.disk.len(),
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

fn extent_node(file_name: &U16CStr) -> OperationResult<ExtentNode> {
    let value = file_name.to_string_lossy().replace('/', "\\");
    let normalized = value.trim_end_matches('\\');
    if normalized.is_empty() {
        return Ok(ExtentNode::Root);
    }
    if normalized.eq_ignore_ascii_case("\\disk.raw") {
        return Ok(ExtentNode::Disk);
    }
    Err(STATUS_OBJECT_NAME_NOT_FOUND)
}

fn forbidden_access(node: ExtentNode, desired_access: winnt::ACCESS_MASK) -> bool {
    let destructive = winnt::GENERIC_ALL | winnt::DELETE | winnt::WRITE_DAC | winnt::WRITE_OWNER;
    let root_write = winnt::GENERIC_WRITE
        | winnt::GENERIC_ALL
        | winnt::FILE_WRITE_DATA
        | winnt::FILE_APPEND_DATA
        | winnt::FILE_WRITE_EA
        | winnt::FILE_WRITE_ATTRIBUTES
        | winnt::FILE_DELETE_CHILD;
    desired_access & destructive != 0
        || (node == ExtentNode::Root && desired_access & root_write != 0)
}

fn require_disk(node: ExtentNode) -> OperationResult<()> {
    (node == ExtentNode::Disk)
        .then_some(())
        .ok_or(STATUS_FILE_IS_A_DIRECTORY)
}

fn bounded_read_length(offset: i64, requested: usize, length: u64) -> OperationResult<usize> {
    if offset < 0 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let available = length.saturating_sub(offset as u64);
    usize::try_from(available.min(requested as u64)).map_err(|_| STATUS_INVALID_BUFFER_SIZE)
}

fn file_information(node: ExtentNode, disk_length: u64) -> FileInfo {
    FileInfo {
        attributes: if node == ExtentNode::Root {
            root_attributes()
        } else {
            disk_attributes()
        },
        creation_time: SystemTime::UNIX_EPOCH,
        last_access_time: SystemTime::UNIX_EPOCH,
        last_write_time: SystemTime::UNIX_EPOCH,
        file_size: if node == ExtentNode::Disk {
            disk_length
        } else {
            0
        },
        number_of_links: 1,
        file_index: if node == ExtentNode::Root {
            ROOT_FILE_INDEX
        } else {
            DISK_FILE_INDEX
        },
    }
}

fn root_attributes() -> u32 {
    winnt::FILE_ATTRIBUTE_DIRECTORY | winnt::FILE_ATTRIBUTE_NOT_CONTENT_INDEXED
}

fn disk_attributes() -> u32 {
    winnt::FILE_ATTRIBUTE_NORMAL | winnt::FILE_ATTRIBUTE_NOT_CONTENT_INDEXED
}

fn map_emulation_error(error: EmulationError) -> NTSTATUS {
    match error {
        EmulationError::OutOfBounds { .. } => STATUS_DISK_FULL,
        EmulationError::WriteTooLarge { .. } => STATUS_INVALID_BUFFER_SIZE,
        EmulationError::ArithmeticOverflow
        | EmulationError::InvalidLogicalLength(_)
        | EmulationError::InvalidClusterSize(_)
        | EmulationError::InvalidExtentPath(_)
        | EmulationError::InvalidVmdkDescriptor(_)
        | EmulationError::InvalidIsoFileName(_)
        | EmulationError::InvalidVmx(_) => STATUS_INVALID_PARAMETER,
        EmulationError::Io(_)
        | EmulationError::ParentRead(_)
        | EmulationError::CorruptOverlay(_)
        | EmulationError::ParentMismatch
        | EmulationError::LockPoisoned
        | EmulationError::OverlayExists(_) => STATUS_IO_DEVICE_ERROR,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/emulation_backend/dokan.rs"]
mod tests;
