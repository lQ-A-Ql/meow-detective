use std::io::Write as _;
use std::path::Path;

use serde_json::{Map, Value};
use zeroize::Zeroizing;

use super::{is_valid_hex, PluginActionError, IMAGE_KEY_ENTRY, IMAGE_XOR_KEY_ENTRY};

/// Persist recovered keys through a temporary file and atomic replacement.
pub(crate) fn write_keys_file(
    path: &Path,
    keys: &Map<String, Value>,
) -> Result<(), PluginActionError> {
    let parent = path.parent().ok_or_else(|| {
        PluginActionError::InvalidInput("keys file path has no parent".to_string())
    })?;
    std::fs::create_dir_all(parent)?;
    let mut merged = read_existing_keys(path)?;
    merged.extend(keys.clone());
    let content = Zeroizing::new(
        serde_json::to_string(&Value::Object(merged))
            .map_err(|error| PluginActionError::Plugin(error.to_string()))?,
    );
    let staging = parent.join(format!(".wechat-keys-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        replace_file(&staging, path)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&staging);
        return Err(error.into());
    }
    Ok(())
}

fn read_existing_keys(path: &Path) -> Result<Map<String, Value>, PluginActionError> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let content = Zeroizing::new(std::fs::read_to_string(path)?);
    let parsed: Value = serde_json::from_str(&content).map_err(|error| {
        PluginActionError::Plugin(format!("existing keys file is invalid: {error}"))
    })?;
    let object = parsed.as_object().ok_or_else(|| {
        PluginActionError::Plugin("existing keys file must be a JSON object".to_string())
    })?;
    Ok(object
        .iter()
        .filter(|(key, value)| {
            value.as_str().is_some_and(|value| match key.as_str() {
                IMAGE_KEY_ENTRY => is_valid_hex(value, 32),
                IMAGE_XOR_KEY_ENTRY => is_valid_hex(value, 2),
                _ => is_valid_hex(value, 64),
            })
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect())
}

#[cfg(windows)]
fn replace_file(staging: &Path, destination: &Path) -> std::io::Result<()> {
    if !destination.exists() {
        return std::fs::rename(staging, destination);
    }
    use std::os::windows::ffi::OsStrExt as _;
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let staging_wide = staging
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are NUL-terminated UTF-16 buffers that remain alive
    // for the call. Optional backup/exclusion parameters are intentionally null.
    let replaced = unsafe {
        windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
            destination_wide.as_ptr(),
            staging_wide.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(staging: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(staging, destination)
}
