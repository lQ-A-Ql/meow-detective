use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use super::dictionary::MAX_DICTIONARY_BYTES;
use super::error::BitLockerServiceError;

pub(super) struct DictionaryIdentity {
    pub sha256: String,
    pub size: u64,
}

pub(super) fn open_and_fingerprint(
    path: &Path,
    cancelled: &AtomicBool,
) -> Result<Option<(File, DictionaryIdentity)>, BitLockerServiceError> {
    let mut file = open_read_only(path).map_err(BitLockerServiceError::DictionaryRead)?;
    let metadata = file
        .metadata()
        .map_err(BitLockerServiceError::DictionaryRead)?;
    if !metadata.is_file() || metadata.len() > MAX_DICTIONARY_BYTES {
        return Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary must be a file no larger than 1 GiB",
        });
    }
    let size = metadata.len();
    let mut hasher = Sha256::new();
    let mut read_bytes = 0u64;
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Ok(None);
        }
        let count = file
            .read(&mut buffer)
            .map_err(BitLockerServiceError::DictionaryRead)?;
        if count == 0 {
            break;
        }
        read_bytes = read_bytes.checked_add(count as u64).ok_or(
            BitLockerServiceError::DictionaryInvalid {
                reason: "dictionary file exceeds the size limit",
            },
        )?;
        if read_bytes > MAX_DICTIONARY_BYTES {
            return Err(BitLockerServiceError::DictionaryInvalid {
                reason: "dictionary file exceeds the size limit",
            });
        }
        hasher.update(&buffer[..count]);
    }
    if read_bytes != size {
        return Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary changed while fingerprinting",
        });
    }
    file.seek(SeekFrom::Start(0))
        .map_err(BitLockerServiceError::DictionaryRead)?;
    Ok(Some((
        file,
        DictionaryIdentity {
            sha256: hex::encode(hasher.finalize()),
            size,
        },
    )))
}

#[cfg(windows)]
fn open_read_only(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001)
        .open(path)
}

#[cfg(not(windows))]
fn open_read_only(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).open(path)
}
