//! Command-line shape shared by the guided run and the explicit
//! subcommands: an optional `--drive <letter>` override that selects the
//! Windows installation root explicitly. Auto-detection
//! (`find_single_windows_installation`) only supports exactly one offline
//! Windows installation; the flag bypasses that limitation.

use std::path::PathBuf;

use crate::MaintenanceError;

/// Removes the optional `--drive <letter>` pair (accepted as `D` or `D:`)
/// from the argument list and returns the normalized installation root.
pub fn split_drive_flag<I>(arguments: I) -> Result<(Vec<String>, Option<PathBuf>), MaintenanceError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments: Vec<String> = arguments.into_iter().collect();
    let Some(position) = arguments.iter().position(|argument| argument == "--drive") else {
        return Ok((arguments, None));
    };
    if position + 2 != arguments.len() {
        return Err(MaintenanceError::InvalidDriveArgument);
    }
    let value = arguments.remove(position + 1);
    arguments.remove(position);
    let mut chars = value.chars();
    let letter = match (chars.next(), chars.next(), chars.next()) {
        (Some(letter), None, None) if letter.is_ascii_alphabetic() => letter,
        (Some(letter), Some(':'), None) if letter.is_ascii_alphabetic() => letter,
        _ => return Err(MaintenanceError::InvalidDriveArgument),
    };
    let root = PathBuf::from(format!("{}:\\", letter.to_ascii_uppercase()));
    Ok((arguments, Some(root)))
}
