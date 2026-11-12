use transport::CommandError;

use super::ceph_bluefs_file_reader::BluefsExtentReader;
use super::ceph_bluefs_replay::{BluefsReplayFile, BluefsReplaySnapshot};

const CURRENT_PATH: &str = "db/CURRENT";
const IDENTITY_PATH: &str = "db/IDENTITY";
const MAX_CURRENT_BYTES: u64 = 4096;
const MAX_IDENTITY_BYTES: u64 = 256;
const MANIFEST_PREFIX: &str = "MANIFEST-";

pub(super) struct RocksdbControlFiles {
    pub(super) manifest_path: String,
    pub(super) manifest_file_number: u64,
    pub(super) manifest_file_size: u64,
    pub(super) identity_uuid: Option<String>,
    pub(super) manifest_bytes: Vec<u8>,
}

pub(super) fn read_rocksdb_control_files(
    reader: &mut BluefsExtentReader<'_>,
    snapshot: &BluefsReplaySnapshot,
) -> Result<RocksdbControlFiles, CommandError> {
    let current = required_file(snapshot, CURRENT_PATH)?;
    validate_small_control_file(current, MAX_CURRENT_BYTES)?;
    let current_bytes = reader.read_plain_file(&current.fnode)?;
    let (manifest_name, manifest_file_number) = parse_current(&current_bytes)?;
    let manifest_path = format!("db/{manifest_name}");
    let manifest = required_file(snapshot, &manifest_path)?;
    let manifest_bytes = reader.read_plain_file(&manifest.fnode)?;
    let identity_uuid = optional_file(snapshot, IDENTITY_PATH)
        .map(|file| {
            validate_small_control_file(file, MAX_IDENTITY_BYTES)?;
            reader
                .read_plain_file(&file.fnode)
                .and_then(|bytes| parse_identity(&bytes))
        })
        .transpose()?;

    Ok(RocksdbControlFiles {
        manifest_path,
        manifest_file_number,
        manifest_file_size: manifest.fnode.size,
        identity_uuid,
        manifest_bytes,
    })
}

fn required_file<'a>(
    snapshot: &'a BluefsReplaySnapshot,
    path: &str,
) -> Result<&'a BluefsReplayFile, CommandError> {
    optional_file(snapshot, path)
        .ok_or_else(|| control_error(format!("BlueFS replay is missing required file {path}")))
}

fn optional_file<'a>(
    snapshot: &'a BluefsReplaySnapshot,
    path: &str,
) -> Option<&'a BluefsReplayFile> {
    snapshot.files.iter().find(|file| file.path == path)
}

fn validate_small_control_file(
    file: &BluefsReplayFile,
    max_bytes: u64,
) -> Result<(), CommandError> {
    if file.fnode.size == 0 || file.fnode.size > max_bytes {
        return Err(control_error(format!(
            "RocksDB control file {} has invalid size {}",
            file.path, file.fnode.size
        )));
    }
    Ok(())
}

fn parse_current(bytes: &[u8]) -> Result<(String, u64), CommandError> {
    if bytes.last() != Some(&b'\n')
        || bytes[..bytes.len().saturating_sub(1)]
            .iter()
            .any(|byte| matches!(byte, b'\r' | b'\n'))
    {
        return Err(control_error(
            "RocksDB CURRENT must contain one descriptor filename followed by LF",
        ));
    }
    let name_bytes = &bytes[..bytes.len() - 1];
    let name = std::str::from_utf8(name_bytes)
        .map_err(|_| control_error("RocksDB CURRENT is not valid UTF-8"))?;
    let digits = name
        .strip_prefix(MANIFEST_PREFIX)
        .filter(|digits| !digits.is_empty())
        .ok_or_else(|| control_error("RocksDB CURRENT does not name a MANIFEST descriptor"))?;
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(control_error(
            "RocksDB CURRENT manifest number is not ASCII decimal",
        ));
    }
    let number = digits
        .parse::<u64>()
        .map_err(|_| control_error("RocksDB CURRENT manifest number overflows u64"))?;
    if number == 0 {
        return Err(control_error(
            "RocksDB CURRENT manifest number must be non-zero",
        ));
    }
    Ok((name.to_string(), number))
}

fn parse_identity(bytes: &[u8]) -> Result<String, CommandError> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| control_error("RocksDB IDENTITY is not valid UTF-8"))?;
    if value.contains(['\r', '\n', '\0']) {
        return Err(control_error(
            "RocksDB IDENTITY must contain one canonical UUID",
        ));
    }
    let uuid = uuid::Uuid::parse_str(value)
        .map_err(|_| control_error("RocksDB IDENTITY is not a UUID"))?;
    if uuid.to_string() != value {
        return Err(control_error(
            "RocksDB IDENTITY UUID is not in canonical lowercase form",
        ));
    }
    Ok(value.to_string())
}

fn control_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("RocksDB control-file validation failed: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_control_files.rs"]
mod tests;
