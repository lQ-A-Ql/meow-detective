use persistence_sqlite::{DbError, DbResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct DirectoryIdentity {
    volume: u64,
    object: u64,
}

pub(super) fn read(path: &Path) -> DbResult<DirectoryIdentity> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || linked(&metadata) {
        return Err(DbError::System(
            "cleanup target is not an ordinary directory".into(),
        ));
    }
    platform_identity(path, &metadata)
}

pub(super) fn linked(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(windows)]
fn platform_identity(path: &Path, _: &std::fs::Metadata) -> DbResult<DirectoryIdentity> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT,
    };
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    // SAFETY: all fields are integer values and zero initializes the output structure.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the open directory handle and output pointer are valid for this call.
    if unsafe { GetFileInformationByHandle(directory.as_raw_handle() as _, &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(DirectoryIdentity {
        volume: u64::from(info.dwVolumeSerialNumber),
        object: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

#[cfg(unix)]
fn platform_identity(_: &Path, metadata: &std::fs::Metadata) -> DbResult<DirectoryIdentity> {
    use std::os::unix::fs::MetadataExt;
    Ok(DirectoryIdentity {
        volume: metadata.dev(),
        object: metadata.ino(),
    })
}

pub(super) fn rename(source: &Path, target: &Path) -> DbResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        let source = source
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let target = target
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // SAFETY: both paths are owned, NUL-terminated UTF-16 buffers valid throughout the call.
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(source, target)?;
    sync_parent(target)
}

pub(super) fn sync_parent(path: &Path) -> DbResult<()> {
    #[cfg(unix)]
    std::fs::File::open(
        path.parent()
            .ok_or_else(|| DbError::System("cleanup path has no parent".into()))?,
    )?
    .sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
