use std::{
    fs::File,
    io,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use sha2::{Digest, Sha256};

pub(super) struct PhysicalScope {
    files: Vec<(File, Vec<u8>)>,
    failed: AtomicBool,
    _permit: crate::resource_budget::ResourcePermit,
}

impl PhysicalScope {
    pub(super) fn acquire(paths: &[PathBuf]) -> io::Result<Self> {
        let permit = crate::resource_budget::global_pool()
            .acquire(
                crate::resource_budget::Resources {
                    handles: paths.len() as u64,
                    bytes: paths.len() as u64 * 4096,
                    ..Default::default()
                },
                None,
            )
            .map_err(crate::resource_budget::ResourceError::into_io)?;
        let files = paths
            .iter()
            .map(|path| {
                let file = File::open(path)?;
                let identity = identity(&file)?;
                Ok((file, identity))
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            files,
            failed: AtomicBool::new(false),
            _permit: permit,
        })
    }

    pub(super) fn fingerprint(&self) -> String {
        let mut digest = Sha256::new();
        for (_, identity) in &self.files {
            digest.update((identity.len() as u64).to_le_bytes());
            digest.update(identity);
        }
        hex::encode(digest.finalize())
    }

    pub(super) fn validate(&self) -> io::Result<()> {
        self.validate_epoch()?;
        for (file, expected) in &self.files {
            if identity(file)? != *expected {
                self.failed.store(true, Ordering::Release);
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "physical source changed during verification",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn validate_epoch(&self) -> io::Result<()> {
        if self.failed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "physical source verification was revoked",
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
fn identity(file: &File) -> io::Result<Vec<u8>> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx, FILE_BASIC_INFO, FILE_ID_INFO,
    };
    let mut basic = FILE_BASIC_INFO::default();
    let mut id = FILE_ID_INFO::default();
    // SAFETY: both buffers have the exact native structure sizes; the file handle remains open.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&mut basic as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of_val(&basic) as u32,
        ) != 0
            && GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileIdInfo,
                (&mut id as *mut FILE_ID_INFO).cast(),
                std::mem::size_of_val(&id) as u32,
            ) != 0
    };
    if !ok {
        return Err(io::Error::last_os_error());
    }
    let mut value = Vec::with_capacity(56);
    value.extend_from_slice(&id.VolumeSerialNumber.to_le_bytes());
    value.extend_from_slice(&id.FileId.Identifier);
    value.extend_from_slice(&basic.ChangeTime.to_le_bytes());
    value.extend_from_slice(&basic.LastWriteTime.to_le_bytes());
    value.extend_from_slice(&file.metadata()?.len().to_le_bytes());
    Ok(value)
}

#[cfg(unix)]
fn identity(file: &File) -> io::Result<Vec<u8>> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok([
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.ctime() as u64,
        metadata.ctime_nsec() as u64,
        metadata.mtime() as u64,
        metadata.mtime_nsec() as u64,
    ]
    .into_iter()
    .flat_map(u64::to_le_bytes)
    .collect())
}
