use crate::{verify_signature, UpdateError};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use transport::dto::PortablePackageDto;

pub const METADATA_MAGIC: &[u8; 16] = b"MEOW-PORTABLE-V1";

pub fn read_metadata(path: &Path) -> Result<PortablePackageDto, UpdateError> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size < 20 {
        return Err(UpdateError::Invalid("missing executable metadata"));
    }
    file.seek(SeekFrom::End(-20))?;
    let mut footer = [0u8; 20];
    file.read_exact(&mut footer)?;
    let length = u32::from_le_bytes(
        footer[..4]
            .try_into()
            .map_err(|_| UpdateError::Invalid("invalid metadata length"))?,
    ) as u64;
    if &footer[4..] != METADATA_MAGIC || length == 0 || length > 4096 || length + 20 > size {
        return Err(UpdateError::Invalid("invalid executable metadata footer"));
    }
    file.seek(SeekFrom::End(-20 - length as i64))?;
    let mut bytes = vec![0u8; length as usize];
    file.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn validate_candidate(
    path: &Path,
    signature: &str,
    key: &str,
    version: &str,
    schema: &str,
) -> Result<(), UpdateError> {
    verify_signature(path, signature, key)?;
    let metadata = read_metadata(path)?;
    if metadata.format != 1 || metadata.version != version || metadata.case_schema != schema {
        return Err(UpdateError::Invalid(
            "signed version or database schema mismatch",
        ));
    }
    let mut header = [0u8; 2];
    File::open(path)?.read_exact(&mut header)?;
    if header != *b"MZ" {
        return Err(UpdateError::Invalid("update is not a Windows executable"));
    }
    Ok(())
}
