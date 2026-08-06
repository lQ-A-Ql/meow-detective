use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use thiserror::Error;

const ISO_SECTOR_SIZE: u64 = 2048;
const FIRST_VOLUME_DESCRIPTOR: u64 = 16;
const MAX_VOLUME_DESCRIPTORS: u64 = 64;
const MAX_RECOVERY_ISO_LENGTH: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub(super) enum RecoveryMediaError {
    #[error("recovery media I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("recovery media must be a regular local ISO file")]
    InvalidFile,
    #[error("recovery media must contain ISO9660 and El Torito boot descriptors")]
    NotBootableIso,
}

pub(super) struct RecoveryMedia {
    vmware_path: String,
    file_name: String,
    length: u64,
    sha256: String,
}

impl RecoveryMedia {
    pub(super) fn open(path: &Path) -> Result<Self, RecoveryMediaError> {
        let path = path.canonicalize()?;
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_file()
            || is_reparse_point(&metadata)
            || !has_iso_extension(&path)
            || metadata.len() < (FIRST_VOLUME_DESCRIPTOR + 3) * ISO_SECTOR_SIZE
            || metadata.len() > MAX_RECOVERY_ISO_LENGTH
        {
            return Err(RecoveryMediaError::InvalidFile);
        }
        validate_boot_descriptors(&path)?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or(RecoveryMediaError::InvalidFile)?
            .to_string();
        let sha256 = infrastructure::hashing::sha256_file(&path)?;
        let vmware_path = vmware_path(&path)?;
        Ok(Self {
            vmware_path,
            file_name,
            length: metadata.len(),
            sha256,
        })
    }

    pub(super) fn vmware_path(&self) -> &str {
        &self.vmware_path
    }

    pub(super) fn file_name(&self) -> &str {
        &self.file_name
    }

    pub(super) fn length(&self) -> u64 {
        self.length
    }

    pub(super) fn sha256(&self) -> &str {
        &self.sha256
    }
}

fn validate_boot_descriptors(path: &Path) -> Result<(), RecoveryMediaError> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(FIRST_VOLUME_DESCRIPTOR * ISO_SECTOR_SIZE))?;
    let mut sector = [0u8; ISO_SECTOR_SIZE as usize];
    let mut saw_primary = false;
    let mut saw_el_torito = false;
    for _ in FIRST_VOLUME_DESCRIPTOR..MAX_VOLUME_DESCRIPTORS {
        file.read_exact(&mut sector)?;
        if &sector[1..6] != b"CD001" || sector[6] != 1 {
            return Err(RecoveryMediaError::NotBootableIso);
        }
        match sector[0] {
            0 => {
                let system_id = &sector[7..39];
                if trim_ascii_spaces(system_id) == b"EL TORITO SPECIFICATION" {
                    saw_el_torito = true;
                }
            }
            1 => saw_primary = true,
            255 => break,
            _ => {}
        }
    }
    if saw_primary && saw_el_torito {
        Ok(())
    } else {
        Err(RecoveryMediaError::NotBootableIso)
    }
}

fn trim_ascii_spaces(value: &[u8]) -> &[u8] {
    let end = value
        .iter()
        .rposition(|byte| *byte != 0 && *byte != b' ')
        .map_or(0, |index| index + 1);
    &value[..end]
}

fn has_iso_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("iso"))
}

fn vmware_path(path: &Path) -> Result<String, RecoveryMediaError> {
    let value = path.to_str().ok_or(RecoveryMediaError::InvalidFile)?;
    let value = value.strip_prefix(r"\\?\").unwrap_or(value);
    let bytes = value.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
    {
        return Err(RecoveryMediaError::InvalidFile);
    }
    Ok(value.to_string())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & winapi::um::winnt::FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
#[path = "../../tests/unit/emulation_registry/recovery_media.rs"]
mod tests;
