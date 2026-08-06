use std::path::PathBuf;

use crate::MaintenanceError;

pub fn ensure_winpe_runtime() -> Result<(), MaintenanceError> {
    let system_drive = std::env::var("SystemDrive").unwrap_or_default();
    if !system_drive.eq_ignore_ascii_case("X:")
        || !PathBuf::from(r"X:\Windows\System32\wpeutil.exe").is_file()
    {
        return Err(MaintenanceError::NotWinPe);
    }
    Ok(())
}

pub fn windows_drive_roots() -> Vec<PathBuf> {
    (b'C'..=b'Z')
        .filter(|letter| *letter != b'X')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .collect()
}
