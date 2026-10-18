use base64::Engine;
use domain::CaseId;
use evidence_core::EvidenceReader;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, Prefix},
};
use transport::{
    dto::{
        CarvedArtifactDto, CarvedExportDto, CarvedExportRequestDto, CarvedRangeDto,
        CarvedRangeRequestDto,
    },
    CommandError,
};

fn reader(
    conn: &Connection,
    case: &CaseId,
    artifact: &CarvedArtifactDto,
) -> Result<Box<dyn EvidenceReader>, CommandError> {
    let factory = super::access::factory(conn, case, &artifact.data_source_id)?;
    let start = super::access::decimal(&artifact.offset)?;
    let length = super::access::decimal(&artifact.length)?;
    if length > 128 * 1024 * 1024
        || start
            .checked_add(length)
            .is_none_or(|end| end > factory.logical_size())
    {
        return Err(CommandError::security(
            "Carving range no longer belongs to its source",
        ));
    }
    factory
        .open_reader()
        .map_err(|_| CommandError::internal("Cannot open carved bytes"))
}
pub fn read_carved_artifact_range(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    request: &CarvedRangeRequestDto,
) -> Result<CarvedRangeDto, CommandError> {
    if !(1..=1024 * 1024).contains(&request.length) {
        return Err(CommandError::invalid_input("Invalid carved range length"));
    }
    let artifact = super::access::load(conn, root, case, &request.artifact_id)?;
    let offset = super::access::decimal(&request.offset)?;
    let length = super::access::decimal(&artifact.length)?;
    if offset > length {
        return Err(CommandError::invalid_input(
            "Carved range offset outside artifact",
        ));
    }
    let amount = u64::from(request.length).min(length - offset) as usize;
    let absolute = super::access::decimal(&artifact.offset)?
        .checked_add(offset)
        .ok_or_else(|| CommandError::invalid_input("Carved range overflow"))?;
    let mut source = reader(conn, case, &artifact)?;
    source.seek(SeekFrom::Start(absolute)).map_err(io_error)?;
    let mut bytes = vec![0; amount];
    source.read_exact(&mut bytes).map_err(io_error)?;
    if artifact.source_revision
        != super::access::source_revision(conn, root, case, &artifact.data_source_id)?
    {
        return Err(CommandError::security("Carving evidence changed"));
    }
    Ok(CarvedRangeDto {
        artifact_id: artifact.artifact_id,
        offset: offset.to_string(),
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        eof: offset + amount as u64 == length,
    })
}
pub fn export_carved_artifact(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    request: &CarvedExportRequestDto,
) -> Result<CarvedExportDto, CommandError> {
    let destination = Path::new(&request.destination_path);
    validate_destination(destination)?;
    let artifact = super::access::load(conn, root, case, &request.artifact_id)?;
    let mut source = reader(conn, case, &artifact)?;
    source
        .seek(SeekFrom::Start(super::access::decimal(&artifact.offset)?))
        .map_err(io_error)?;
    let length = super::access::decimal(&artifact.length)?;
    let parent = destination
        .parent()
        .ok_or_else(|| CommandError::invalid_input("Export parent missing"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut remaining = length;
    let mut bytes = [0u8; 65536];
    while remaining > 0 {
        let amount = remaining.min(bytes.len() as u64) as usize;
        source.read_exact(&mut bytes[..amount]).map_err(io_error)?;
        temporary.write_all(&bytes[..amount]).map_err(io_error)?;
        hash.update(&bytes[..amount]);
        remaining -= amount as u64;
    }
    let digest = hex::encode(hash.finalize());
    if digest != artifact.sha256
        || artifact.source_revision
            != super::access::source_revision(conn, root, case, &artifact.data_source_id)?
    {
        return Err(CommandError::security(
            "Exported carving bytes failed integrity verification",
        ));
    }
    temporary.as_file().sync_all().map_err(io_error)?;
    temporary.persist_noclobber(destination).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            CommandError::conflict("Export target exists")
        } else {
            io_error(e.error)
        }
    })?;
    Ok(CarvedExportDto {
        artifact_id: artifact.artifact_id,
        bytes_written: length.to_string(),
        sha256: digest,
    })
}
fn io_error(_: std::io::Error) -> CommandError {
    CommandError::internal("Carving byte I/O failed")
}
fn validate_destination(path: &Path) -> Result<(), CommandError> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| CommandError::invalid_input("Invalid export filename"))?;
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (1..=9).any(|i| stem == format!("COM{i}") || stem == format!("LPT{i}"));
    if !path.is_absolute()
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || reserved
        || path.parent().is_none_or(|p| !p.is_dir())
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(CommandError::invalid_input("Invalid carving export target"));
    }
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        if !matches!(
            prefix.kind(),
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) | Prefix::UNC(_, _)
        ) {
            return Err(CommandError::invalid_input(
                "Device export paths are unsupported",
            ));
        }
    }
    if path.try_exists().map_err(io_error)? {
        return Err(CommandError::conflict("Export target exists"));
    }
    Ok(())
}
