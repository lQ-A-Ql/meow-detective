use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MaintenanceError {
    #[error("maintenance I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("the maintenance helper is not running inside WinPE")]
    NotWinPe,
    #[error("no offline Windows installation was found")]
    WindowsInstallationMissing,
    #[error("multiple offline Windows installations were found")]
    MultipleWindowsInstallations,
    #[error("the selected root is not an offline Windows installation")]
    InvalidWindowsInstallation,
    #[error("OSDATA is a reparse point and cannot be removed safely")]
    OsdataReparsePoint,
    #[error("OSDATA is a non-empty directory and cannot be removed automatically")]
    OsdataDirectoryNotEmpty,
    #[error("OSDATA has an unsupported filesystem node type")]
    UnsupportedOsdataNode,
    #[error("the utilman bypass backup already exists")]
    BypassBackupExists,
    #[error("the utilman bypass backup is missing")]
    BypassBackupMissing,
    #[error("utilman.exe or cmd.exe is missing or not a regular file")]
    BypassTargetMissing,
    #[error("maintenance targets manifest is invalid: {0}")]
    InvalidTargets(String),
}
