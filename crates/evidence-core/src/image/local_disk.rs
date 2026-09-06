//! Read-only access to Windows host physical disks.

use crate::reader::{EvidenceReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MAX_LOCAL_DISK_INDEX: u32 = 64;
const READ_CACHE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDiskInfo {
    pub path: PathBuf,
    pub disk_number: u32,
    pub size: u64,
}

#[derive(Debug)]
pub struct LocalDiskReader {
    file: std::fs::File,
    info: ReaderInfo,
    cursor: u64,
    cache: Vec<u8>,
    cache_start: u64,
    cache_len: usize,
}

impl LocalDiskReader {
    pub fn is_supported_path(path: &Path) -> bool {
        parse_physical_drive_path(path).is_some()
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        let disk_number = parse_physical_drive_path(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "local disk path must be \\\\.\\PhysicalDriveN",
            )
        })?;
        open_local_disk(path, disk_number)
    }

    pub fn len(&self) -> u64 {
        self.info.size
    }

    pub fn is_empty(&self) -> bool {
        self.info.size == 0
    }

    pub fn path(&self) -> &Path {
        &self.info.path
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            file: self.file.try_clone()?,
            info: self.info.clone(),
            cursor: self.cursor,
            cache: Vec::new(),
            cache_start: 0,
            cache_len: 0,
        })
    }
}

impl Read for LocalDiskReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cursor >= self.info.size || buf.is_empty() {
            return Ok(0);
        }
        let requested = (self.info.size - self.cursor).min(buf.len() as u64) as usize;
        let read = if requested >= READ_CACHE_BYTES / 2 {
            read_at(&self.file, self.cursor, &mut buf[..requested])?
        } else {
            self.read_cached(&mut buf[..requested])?
        };
        self.cursor = self.cursor.saturating_add(read as u64);
        Ok(read)
    }
}

impl LocalDiskReader {
    fn read_cached(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let end = self.cursor.saturating_add(buffer.len() as u64);
        let cache_end = self.cache_start.saturating_add(self.cache_len as u64);
        if self.cursor < self.cache_start || end > cache_end {
            let aligned_start = self.cursor / READ_CACHE_BYTES as u64 * READ_CACHE_BYTES as u64;
            let available = self.info.size.saturating_sub(aligned_start);
            let fill_len = available.min(READ_CACHE_BYTES as u64) as usize;
            self.cache.resize(fill_len, 0);
            self.cache_len = read_at(&self.file, aligned_start, &mut self.cache)?;
            self.cache_start = aligned_start;
            if self.cache_len == 0 {
                return Ok(0);
            }
        }
        let start = (self.cursor - self.cache_start) as usize;
        let available = self.cache_len.saturating_sub(start);
        let copied = available.min(buffer.len());
        buffer[..copied].copy_from_slice(&self.cache[start..start + copied]);
        Ok(copied)
    }
}

impl Seek for LocalDiskReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.cursor) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.info.size) + i128::from(offset),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek position is outside the addressable local disk range",
            ));
        }
        self.cursor = next as u64;
        Ok(self.cursor)
    }
}

impl EvidenceReader for LocalDiskReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        4096
    }
}

pub fn list_local_disks() -> io::Result<Vec<LocalDiskInfo>> {
    #[cfg(windows)]
    {
        let mut disks = Vec::new();
        for disk_number in 0..MAX_LOCAL_DISK_INDEX {
            let path = physical_drive_path(disk_number);
            match open_local_disk(&path, disk_number) {
                Ok(reader) => disks.push(LocalDiskInfo {
                    path,
                    disk_number,
                    size: reader.len(),
                }),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                    ) => {}
                Err(_error) => {}
            }
        }
        Ok(disks)
    }
    #[cfg(not(windows))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "local physical disk access is only supported on Windows",
        ))
    }
}

fn open_local_disk(path: &Path, disk_number: u32) -> io::Result<LocalDiskReader> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(windows_disk::FILE_SHARE_READ | windows_disk::FILE_SHARE_WRITE)
            .open(path)?;
        let size = windows_disk::disk_length(&file)?;
        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("PhysicalDrive{disk_number} reported zero capacity"),
            ));
        }
        Ok(LocalDiskReader {
            file,
            info: ReaderInfo {
                path: path.to_path_buf(),
                size,
                kind: "local_disk".to_string(),
            },
            cursor: 0,
            cache: Vec::new(),
            cache_start: 0,
            cache_len: 0,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (path, disk_number);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "local physical disk access is only supported on Windows",
        ))
    }
}

#[cfg(windows)]
mod windows_disk {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::io::AsRawHandle;

    const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007_405c;
    pub(super) const FILE_SHARE_READ: u32 = 0x0000_0001;
    pub(super) const FILE_SHARE_WRITE: u32 = 0x0000_0002;

    #[repr(C)]
    struct GetLengthInformation {
        length: i64,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            device: *mut c_void,
            control_code: u32,
            input: *const c_void,
            input_len: u32,
            output: *mut c_void,
            output_len: u32,
            returned_len: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }

    pub(super) fn disk_length(file: &std::fs::File) -> io::Result<u64> {
        let mut output = GetLengthInformation { length: 0 };
        let mut returned_len = 0u32;
        // SAFETY: the handle remains valid for the call; all pointers refer to initialized,
        // writable storage with lengths matching the Windows API contract.
        let result = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                IOCTL_DISK_GET_LENGTH_INFO,
                std::ptr::null(),
                0,
                (&mut output as *mut GetLengthInformation).cast(),
                std::mem::size_of::<GetLengthInformation>() as u32,
                &mut returned_len,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        u64::try_from(output.length).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "physical disk length does not fit in u64",
            )
        })
    }
}

fn parse_physical_drive_path(path: &Path) -> Option<u32> {
    let text = path.to_str()?.replace('/', "\\");
    let prefix = r"\\.\PhysicalDrive";
    let head = text.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }
    let suffix = &text[prefix.len()..];
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn physical_drive_path(disk_number: u32) -> PathBuf {
    PathBuf::from(format!(r"\\.\PhysicalDrive{disk_number}"))
}

fn read_at(file: &std::fs::File, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        file.seek_read(buffer, offset)
    }
    #[cfg(not(windows))]
    {
        let mut file = file.try_clone()?;
        file.seek(SeekFrom::Start(offset))?;
        file.read(buffer)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/image/local_disk.rs"]
mod tests;
