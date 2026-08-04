use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::SystemTime;

use dokan::{FillDataError, FillDataResult, FindData, OperationResult};
use evidence_mount::{MountError, MountNode, MountPath, MountSession};
use widestring::{U16CStr, U16CString};
use winapi::shared::{ntdef::NTSTATUS, ntstatus::*};
use winapi::um::winnt;

pub(super) fn path_from_dokan(file_name: &U16CStr) -> OperationResult<MountPath> {
    MountPath::parse(&file_name.to_string_lossy()).map_err(map_mount_error)
}

pub(super) fn has_write_access(desired_access: winnt::ACCESS_MASK) -> bool {
    let write_mask = winnt::GENERIC_WRITE
        | winnt::GENERIC_ALL
        | winnt::FILE_WRITE_DATA
        | winnt::FILE_APPEND_DATA
        | winnt::FILE_WRITE_EA
        | winnt::FILE_WRITE_ATTRIBUTES
        | winnt::FILE_DELETE_CHILD
        | winnt::WRITE_DAC
        | winnt::WRITE_OWNER
        | winnt::DELETE;
    desired_access & write_mask != 0
}

pub(super) fn file_attributes(node: &MountNode) -> u32 {
    let mut attributes = winnt::FILE_ATTRIBUTE_READONLY;
    if node.is_dir {
        attributes |= winnt::FILE_ATTRIBUTE_DIRECTORY;
    }
    if node.hidden {
        attributes |= winnt::FILE_ATTRIBUTE_HIDDEN;
    }
    if node.system {
        attributes |= winnt::FILE_ATTRIBUTE_SYSTEM;
    }
    attributes
}

pub(super) fn stable_file_index(path: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn windows_filesystem_name(source_kind: &str) -> OperationResult<U16CString> {
    let name =
        if source_kind.eq_ignore_ascii_case("fat") || source_kind.eq_ignore_ascii_case("fat32") {
            "FAT"
        } else if source_kind.eq_ignore_ascii_case("exfat") {
            "exFAT"
        } else {
            // Windows file APIs need a well-known name for virtual path handling.
            "NTFS"
        };
    U16CString::from_str(name).map_err(|_| STATUS_OBJECT_NAME_INVALID)
}

pub(super) fn find_directory_files(
    session: &MountSession,
    file_name: &U16CStr,
    fill_find_data: &mut impl FnMut(&FindData) -> FillDataResult,
) -> OperationResult<()> {
    let path = path_from_dokan(file_name)?;
    let mut cursor = None;
    loop {
        let page = session
            .read_directory(&path, cursor.as_deref(), 4096)
            .map_err(map_mount_error)?;
        for node in page.entries {
            let file_name =
                U16CString::from_str(&node.name).map_err(|_| STATUS_OBJECT_NAME_INVALID)?;
            let data = FindData {
                attributes: file_attributes(&node),
                creation_time: node.created_at.unwrap_or(SystemTime::UNIX_EPOCH),
                last_access_time: node.accessed_at.unwrap_or(SystemTime::UNIX_EPOCH),
                last_write_time: node.modified_at.unwrap_or(SystemTime::UNIX_EPOCH),
                file_size: node.size,
                file_name,
            };
            match fill_find_data(&data) {
                Ok(()) | Err(FillDataError::NameTooLong) => {}
                Err(FillDataError::BufferFull) => return Err(STATUS_BUFFER_OVERFLOW),
            }
        }
        let Some(next_cursor) = page.next_cursor else {
            return Ok(());
        };
        if cursor.as_deref() == Some(next_cursor.as_str()) {
            return Err(STATUS_INTERNAL_ERROR);
        }
        cursor = Some(next_cursor);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mount_backend/dokan_support.rs"]
mod tests;

pub(super) fn map_mount_error(error: MountError) -> NTSTATUS {
    match error {
        MountError::NotFound(_) => STATUS_OBJECT_NAME_NOT_FOUND,
        MountError::PathTraversal | MountError::InvalidPath(_) => STATUS_OBJECT_NAME_INVALID,
        MountError::IsDirectory(_) => STATUS_FILE_IS_A_DIRECTORY,
        MountError::NotDirectory(_) => STATUS_NOT_A_DIRECTORY,
        MountError::WriteDenied => STATUS_ACCESS_DENIED,
        MountError::ReadLimit { .. }
        | MountError::OffsetOutOfBounds { .. }
        | MountError::InvalidCursor
        | MountError::InvalidDirectoryLimit => STATUS_INVALID_PARAMETER,
        MountError::HandleLimit => STATUS_TOO_MANY_OPENED_FILES,
        MountError::HandleNotFound(_) => STATUS_INVALID_HANDLE,
        MountError::InvalidPlan(_)
        | MountError::Filesystem(_)
        | MountError::DirectoryLimit { .. } => STATUS_IO_DEVICE_ERROR,
    }
}
