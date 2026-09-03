use crate::reader::{EvidenceReader, ReaderInfo};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

const VMDK_SECTOR_SIZE: u64 = 512;
const MAX_DESCRIPTOR_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
struct Backend {
    file: File,
    base_offset: u64,
}

/// Read-only reader for raw/dd images and supported monolithic-flat VMDKs.
///
/// VMDK support intentionally covers descriptor files which reference one
/// zero-offset `FLAT` extent (`createType="monolithicFlat"`). Sparse,
/// compressed, snapshot and parent-chain descriptors are rejected rather than
/// guessed. The descriptor path remains the provenance path in `ReaderInfo`.
#[derive(Debug)]
pub struct RawImageReader {
    backend: Backend,
    info: ReaderInfo,
    cursor: u64,
    backing_paths: Vec<PathBuf>,
}

impl RawImageReader {
    /// Opens a raw image or a supported monolithic-flat VMDK descriptor.
    pub fn open(path: &Path) -> io::Result<Self> {
        reject_split_raw_member(path)?;
        let descriptor = read_vmdk_descriptor(path)?;
        let (backend, logical_len, kind, backing_paths) = match descriptor {
            Some((extent, sector_count)) => {
                let file = File::open(&extent)?;
                let extent_len = file.metadata()?.len();
                let logical_len = sector_count
                    .checked_mul(VMDK_SECTOR_SIZE)
                    .ok_or_else(|| invalid_data("VMDK logical size overflows u64"))?;
                if logical_len == 0 {
                    return Err(invalid_data("VMDK extent has zero logical sectors"));
                }
                if extent_len < logical_len {
                    return Err(invalid_data(format!(
                        "VMDK FLAT extent is truncated: expected at least {logical_len} bytes, found {extent_len}"
                    )));
                }
                (
                    Backend {
                        file,
                        base_offset: 0,
                    },
                    logical_len,
                    "vmdk",
                    vec![path.to_path_buf(), extent],
                )
            }
            None => {
                let file = File::open(path)?;
                let metadata = file.metadata()?;
                if metadata.is_dir() {
                    return Err(invalid_data(format!(
                        "cannot open directory as raw image: {}",
                        path.display()
                    )));
                }
                (
                    Backend {
                        file,
                        base_offset: 0,
                    },
                    metadata.len(),
                    "raw",
                    vec![path.to_path_buf()],
                )
            }
        };

        Ok(Self {
            backend,
            info: ReaderInfo {
                path: path.to_path_buf(),
                size: logical_len,
                kind: kind.to_string(),
            },
            cursor: 0,
            backing_paths,
        })
    }

    pub fn path(&self) -> &Path {
        &self.info.path
    }

    pub fn len(&self) -> u64 {
        self.info.size
    }

    pub fn is_empty(&self) -> bool {
        self.info.size == 0
    }

    /// Files whose bytes define this image, in stable manifest order.
    pub fn backing_paths(&self) -> &[PathBuf] {
        &self.backing_paths
    }

    /// Creates an independent handle with its own logical cursor.
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            backend: Backend {
                file: self.backend.file.try_clone()?,
                base_offset: self.backend.base_offset,
            },
            info: self.info.clone(),
            cursor: self.cursor,
            backing_paths: self.backing_paths.clone(),
        })
    }
}

impl Read for RawImageReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cursor >= self.info.size || buf.is_empty() {
            return Ok(0);
        }
        let available = self.info.size - self.cursor;
        let requested = available.min(buf.len() as u64) as usize;
        let physical = self
            .backend
            .base_offset
            .checked_add(self.cursor)
            .ok_or_else(|| invalid_data("image read offset overflows u64"))?;
        self.backend.file.seek(SeekFrom::Start(physical))?;
        let read = self.backend.file.read(&mut buf[..requested])?;
        self.cursor = self.cursor.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for RawImageReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.cursor) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.info.size) + i128::from(offset),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek position is outside the addressable image range",
            ));
        }
        self.cursor = next as u64;
        Ok(self.cursor)
    }
}

impl EvidenceReader for RawImageReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn reject_split_raw_member(path: &Path) -> io::Result<()> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if extension.len() != 3 || !extension.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(());
    }
    let index = extension.parse::<u16>().unwrap_or(0);
    let belongs_to_split_set = (index == 1 && path.with_extension("002").is_file())
        || (index > 1 && path.with_extension("001").is_file());
    if belongs_to_split_set {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "split RAW image sets are unsupported; provide a single-file image",
        ));
    }
    Ok(())
}

fn read_vmdk_descriptor(path: &Path) -> io::Result<Option<(PathBuf, u64)>> {
    let Some(text) = read_descriptor_candidate(path)? else {
        if has_vmdk_extension(path) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "VMDK descriptor or supported container was not recognized",
            ));
        }
        return Ok(None);
    };
    if !text
        .lines()
        .any(|line| line.trim() == "# Disk DescriptorFile")
    {
        if has_vmdk_extension(path) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "VMDK descriptor or supported container was not recognized",
            ));
        }
        return Ok(None);
    }
    parse_vmdk_descriptor(path, &text).map(Some)
}

fn has_vmdk_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("vmdk"))
}

fn read_descriptor_candidate(path: &Path) -> io::Result<Option<String>> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    if metadata.is_dir() {
        return Err(invalid_data(format!(
            "cannot open directory as raw image: {}",
            path.display()
        )));
    }
    let mut magic = [0u8; 4];
    let magic_len = file.read(&mut magic)?;
    file.seek(SeekFrom::Start(0))?;
    if magic_len == magic.len() && matches!(&magic, b"KDMV" | b"COWD") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "sparse and stream-optimized VMDK containers are unsupported",
        ));
    }
    if metadata.len() == 0 || metadata.len() > MAX_DESCRIPTOR_BYTES {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_DESCRIPTOR_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    Ok(Some(text))
}

fn parse_vmdk_descriptor(path: &Path, text: &str) -> io::Result<(PathBuf, u64)> {
    let create_type = required_descriptor_value(text, "createType")?;
    if create_type != "\"monolithicFlat\"" {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported VMDK createType {create_type}"),
        ));
    }
    let parent_cid = required_descriptor_value(text, "parentCID")?;
    if !parent_cid.eq_ignore_ascii_case("ffffffff")
        || descriptor_values(text, "parentFileNameHint")
            .next()
            .is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "VMDK parent and snapshot chains are unsupported",
        ));
    }
    let mut extents = text.lines().filter(|line| is_extent(line));
    let extent_line = extents
        .next()
        .ok_or_else(|| invalid_data("VMDK descriptor is missing an extent"))?;
    if extents.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "VMDK descriptors with multiple extents are unsupported",
        ));
    }
    if !is_rw_extent(extent_line) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "VMDK extent access mode must be RW",
        ));
    }
    let (extent_name, sector_count) = parse_flat_extent(extent_line)?;
    let extent = resolve_extent_path(path, extent_name)?;
    Ok((extent, sector_count))
}

fn is_extent(line: &str) -> bool {
    matches!(
        line.split_whitespace().next(),
        Some("RW" | "RDONLY" | "NOACCESS")
    )
}

fn is_rw_extent(line: &str) -> bool {
    line.trim_start()
        .strip_prefix("RW")
        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

fn descriptor_values<'a>(text: &'a str, key: &'a str) -> impl Iterator<Item = &'a str> {
    text.lines().filter_map(move |line| {
        let (candidate, value) = line.trim().split_once('=')?;
        (candidate.trim() == key).then(|| value.trim())
    })
}

fn required_descriptor_value<'a>(text: &'a str, key: &'a str) -> io::Result<&'a str> {
    let mut values = descriptor_values(text, key);
    let value = values
        .next()
        .ok_or_else(|| invalid_data(format!("VMDK descriptor is missing {key}")))?;
    if values.next().is_some() {
        return Err(invalid_data(format!(
            "VMDK descriptor contains duplicate {key} fields"
        )));
    }
    Ok(value)
}

fn parse_flat_extent(extent_line: &str) -> io::Result<(&str, u64)> {
    let line = extent_line.trim();
    let rest = line
        .strip_prefix("RW")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .ok_or_else(|| invalid_data("VMDK extent declaration is invalid"))?
        .trim_start();
    let sector_end = rest
        .find(char::is_whitespace)
        .ok_or_else(|| invalid_data("VMDK extent sector count is missing"))?;
    let sector_text = &rest[..sector_end];
    let extent_spec = rest[sector_end..].trim_start();
    let remainder = extent_spec
        .strip_prefix("FLAT")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "VMDK descriptor must contain one FLAT extent",
            )
        })?
        .trim_start();
    let quoted = remainder
        .strip_prefix('"')
        .ok_or_else(|| invalid_data("VMDK extent path must be quoted"))?;
    let quote_end = quoted
        .find('"')
        .ok_or_else(|| invalid_data("VMDK extent path must be quoted"))?;
    let extent_name = &quoted[..quote_end];
    if quoted[quote_end + 1..].trim() != "0" {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "VMDK FLAT extents with non-zero offsets are unsupported",
        ));
    }
    let sector_count = sector_text
        .parse::<u64>()
        .map_err(|_| invalid_data("VMDK extent sector count is invalid"))?;
    if extent_name.is_empty() || sector_count == 0 {
        return Err(invalid_data("VMDK extent sector count must be non-zero"));
    }
    Ok((extent_name, sector_count))
}

fn resolve_extent_path(descriptor: &Path, extent_name: &str) -> io::Result<PathBuf> {
    if extent_name.is_empty() || extent_name.contains(['\r', '\n', '"']) {
        return Err(invalid_data("VMDK extent path is invalid"));
    }
    let normalized = extent_name.replace('\\', "/");
    let relative = Path::new(&normalized);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "VMDK extent path must remain relative to the descriptor",
        ));
    }
    let parent = descriptor
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let joined = parent.join(relative);
    let canonical_parent = parent;
    let canonical_extent = joined.canonicalize()?;
    if !canonical_extent.starts_with(&canonical_parent) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "VMDK extent resolves outside the descriptor directory",
        ));
    }
    if canonical_extent == descriptor.canonicalize()? {
        return Err(invalid_data(
            "VMDK descriptor cannot also be its own FLAT extent",
        ));
    }
    Ok(canonical_extent)
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
#[path = "../../tests/unit/image/raw_reader.rs"]
mod tests;
