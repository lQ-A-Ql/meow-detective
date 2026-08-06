use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::sys::{clear_readonly, is_reparse_point};
use crate::MaintenanceError;

const SYSTEM_HIVE: &str = "Windows/System32/config/SYSTEM";
const OSDATA: &str = "Windows/System32/config/OSDATA";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsdataState {
    Missing,
    File,
    EmptyDirectory,
    NonEmptyDirectory,
}

pub fn find_single_windows_installation<I>(roots: I) -> Result<PathBuf, MaintenanceError>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut matches = Vec::new();
    for root in roots {
        match fs::metadata(root.join(SYSTEM_HIVE)) {
            Ok(metadata) if metadata.is_file() => matches.push(root),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    match matches.len() {
        0 => Err(MaintenanceError::WindowsInstallationMissing),
        1 => Ok(matches.remove(0)),
        _ => Err(MaintenanceError::MultipleWindowsInstallations),
    }
}

pub fn inspect_osdata(windows_root: &Path) -> Result<OsdataState, MaintenanceError> {
    validate_windows_root(windows_root)?;
    let target = windows_root.join(OSDATA);
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(OsdataState::Missing),
        Err(error) => return Err(error.into()),
    };
    if is_reparse_point(&metadata) {
        return Err(MaintenanceError::OsdataReparsePoint);
    }
    if metadata.is_file() {
        return Ok(OsdataState::File);
    }
    if metadata.is_dir() {
        return if fs::read_dir(target)?.next().is_none() {
            Ok(OsdataState::EmptyDirectory)
        } else {
            Ok(OsdataState::NonEmptyDirectory)
        };
    }
    Err(MaintenanceError::UnsupportedOsdataNode)
}

pub fn remove_osdata(windows_root: &Path) -> Result<OsdataState, MaintenanceError> {
    let state = inspect_osdata(windows_root)?;
    let target = windows_root.join(OSDATA);
    match state {
        OsdataState::Missing => return Ok(state),
        OsdataState::NonEmptyDirectory => return Err(MaintenanceError::OsdataDirectoryNotEmpty),
        OsdataState::File | OsdataState::EmptyDirectory => {}
    }
    clear_readonly(&target)?;
    match state {
        OsdataState::File => fs::remove_file(&target)?,
        OsdataState::EmptyDirectory => fs::remove_dir(&target)?,
        OsdataState::Missing | OsdataState::NonEmptyDirectory => unreachable!(),
    }
    match fs::symlink_metadata(target) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(state),
        Err(error) => Err(error.into()),
        Ok(_) => Err(MaintenanceError::UnsupportedOsdataNode),
    }
}

fn validate_windows_root(windows_root: &Path) -> Result<(), MaintenanceError> {
    if windows_root.join(SYSTEM_HIVE).is_file() {
        Ok(())
    } else {
        Err(MaintenanceError::InvalidWindowsInstallation)
    }
}
