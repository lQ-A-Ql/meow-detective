//! Opening a target: an image file or a device, read-only or writable,
//! optionally `--offset` bytes in (a partition inside a whole-disk image).

use std::ffi::OsString;
use std::sync::Arc;

use fs_btrfs::{Filesystem, Superblock};
use fs_core::cli::CliError;
use fs_core::{BlockDevice, BlockRead, FileDevice, OwnedRwSlice, OwnedSlice};

fn offset_past_end(target: &OsString, offset: u64, size: u64) -> CliError {
    CliError::failed(format!(
        "--offset {offset} is past the end of {} ({size} bytes)",
        target.to_string_lossy()
    ))
}

/// Open `target` read-only, `offset` bytes in.
pub fn open(target: &OsString, offset: u64) -> Result<Arc<dyn BlockRead>, CliError> {
    let name = target.to_string_lossy();
    let dev =
        FileDevice::open(&*name).map_err(|e| CliError::failed(format!("open {name}: {e}")))?;
    let size = dev.size_bytes();
    if offset == 0 {
        return Ok(Arc::new(dev));
    }
    if offset >= size {
        return Err(offset_past_end(target, offset, size));
    }
    Ok(Arc::new(OwnedSlice::new(
        Arc::new(dev),
        offset,
        size - offset,
    )))
}

/// Open `target` read-write, `offset` bytes in.
pub fn open_rw(target: &OsString, offset: u64) -> Result<Arc<dyn BlockDevice>, CliError> {
    let name = target.to_string_lossy();
    let dev = FileDevice::open_rw(&*name)
        .map_err(|e| CliError::failed(format!("open {name} read-write: {e}")))?;
    let size = dev.size_bytes();
    if offset == 0 {
        return Ok(Arc::new(dev));
    }
    if offset >= size {
        return Err(offset_past_end(target, offset, size));
    }
    Ok(Arc::new(OwnedRwSlice::new(
        Arc::new(dev),
        offset,
        size - offset,
    )))
}

/// Open and mount `target` read-only.
pub fn mount(target: &OsString, offset: u64) -> Result<Filesystem, CliError> {
    Filesystem::mount(open(target, offset)?).map_err(|e| {
        CliError::failed(format!(
            "{} is not a readable Btrfs filesystem: {e}",
            target.to_string_lossy()
        ))
    })
}

/// The superblock `target` would be mounted with, without mounting it.
pub fn superblock(target: &OsString, offset: u64) -> Result<Superblock, CliError> {
    let dev = open(target, offset)?;
    fs_btrfs::superblock::read_superblock(&*dev)
        .map(|(sb, _copy)| sb)
        .map_err(|e| {
            CliError::failed(format!(
                "{} has no readable Btrfs superblock: {e}",
                target.to_string_lossy()
            ))
        })
}
