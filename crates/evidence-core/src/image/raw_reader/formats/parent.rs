use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use super::vhdx_guid;

pub(crate) fn read_qcow_backing_path(path: &Path) -> io::Result<Option<PathBuf>> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 72];
    let count = file.read(&mut header)?;
    if count < 32 || &header[..4] != b"QFI\xfb" {
        return Ok(None);
    }
    let backing_offset = u64::from_be_bytes(header[8..16].try_into().unwrap());
    let backing_size = u32::from_be_bytes(header[16..20].try_into().unwrap());
    if backing_offset == 0 && backing_size == 0 {
        return Ok(None);
    }
    if backing_offset == 0 || backing_size == 0 || backing_size > 1024 * 1024 {
        return Err(invalid_data("QCOW backing file reference is invalid"));
    }
    file.seek(SeekFrom::Start(backing_offset))?;
    let mut bytes = vec![0u8; backing_size as usize];
    file.read_exact(&mut bytes)?;
    let name = std::str::from_utf8(&bytes)
        .map_err(|_| invalid_data("QCOW backing file reference is not UTF-8"))?
        .trim_end_matches('\0')
        .trim();
    if name.is_empty() {
        return Err(invalid_data("QCOW backing file reference is empty"));
    }
    resolve_image_parent_path(path, name).map(Some)
}

pub(crate) fn read_vhdx_parent_path(path: &Path) -> io::Result<Option<PathBuf>> {
    let mut file = File::open(path)?;
    let mut signature = [0u8; 8];
    if file.read_exact(&mut signature).is_err() || &signature != b"vhdxfile" {
        return Ok(None);
    }
    let metadata_offset = read_vhdx_metadata_offset(&mut file)?;
    let mut table = vec![0u8; 64 * 1024];
    file.seek(SeekFrom::Start(metadata_offset))?;
    file.read_exact(&mut table)?;
    if &table[..8] != b"metadata" {
        return Err(invalid_data("invalid VHDX metadata table"));
    }
    let count = u16::from_le_bytes(table[10..12].try_into().unwrap()) as usize;
    let parent_guid = vhdx_guid(
        0xa8d35f2d,
        0xb30b,
        0x454d,
        [0xab, 0xf7, 0xd3, 0xd8, 0x48, 0x34, 0xab, 0x00],
    );
    for index in 0..count.min(2047) {
        let start = 32 + index * 32;
        if start + 32 > table.len() || table[start..start + 16] != parent_guid {
            continue;
        }
        let rel = u32::from_le_bytes(table[start + 16..start + 20].try_into().unwrap()) as u64;
        let len = u32::from_le_bytes(table[start + 20..start + 24].try_into().unwrap()) as usize;
        let mut payload = vec![0u8; len.min(64 * 1024)];
        file.seek(SeekFrom::Start(metadata_offset + rel))?;
        file.read_exact(&mut payload)?;
        if let Some(name) = decode_vhdx_parent_name(&payload) {
            return resolve_image_parent_path(path, &name).map(Some);
        }
        return Err(invalid_data("VHDX parent locator does not contain a path"));
    }
    Ok(None)
}

fn read_vhdx_metadata_offset(file: &mut File) -> io::Result<u64> {
    let mut region = vec![0u8; 64 * 1024];
    file.seek(SeekFrom::Start(192 * 1024))?;
    file.read_exact(&mut region)?;
    if &region[..4] != b"regi" {
        return Err(invalid_data("invalid VHDX region table"));
    }
    let count = u32::from_le_bytes(region[8..12].try_into().unwrap()) as usize;
    let metadata_guid = vhdx_guid(
        0x8b7ca206,
        0x4790,
        0x4b9a,
        [0xb8, 0xfe, 0x57, 0x5f, 0x05, 0x0f, 0x88, 0x6e],
    );
    for index in 0..count.min(2047) {
        let start = 16 + index * 32;
        if start + 32 > region.len() || region[start..start + 16] != metadata_guid {
            continue;
        }
        return Ok(u64::from_le_bytes(
            region[start + 16..start + 24].try_into().unwrap(),
        ));
    }
    Err(invalid_data("VHDX metadata region missing"))
}

fn decode_vhdx_parent_name(payload: &[u8]) -> Option<String> {
    let units: Vec<u16> = payload
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
        .split('\0')
        .map(str::trim)
        .find(|value| {
            !value.is_empty()
                && value.len() <= 1024
                && (value.contains('\\')
                    || value.contains('/')
                    || value.to_ascii_lowercase().ends_with(".vhdx")
                    || value.to_ascii_lowercase().ends_with(".vhd"))
        })
        .map(str::to_string)
}

pub(crate) fn read_vhd_parent_name(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len < 512 {
        return Err(invalid_data("VHD parent footer is missing"));
    }
    file.seek(SeekFrom::End(-512))?;
    let mut footer = [0u8; 512];
    file.read_exact(&mut footer)?;
    let value: String = footer[64..]
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .take(224)
        .filter(|unit| *unit != 0)
        .filter_map(|unit| char::from_u32(u32::from(unit)))
        .collect();
    if value.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "differencing VHD has no parent locator",
        ));
    }
    Ok(value)
}

pub(crate) fn resolve_vhd_parent_path(child: &Path, name: &str) -> io::Result<PathBuf> {
    resolve_image_parent_path(child, name)
}

pub(crate) fn resolve_image_parent_path(child: &Path, name: &str) -> io::Result<PathBuf> {
    let normalized = name.replace('\\', "/");
    let relative = Path::new(&normalized);
    if relative
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "virtual disk parent path contains traversal",
        ));
    }
    let parent = child
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let resolved = if relative.is_absolute() {
        relative.canonicalize()?
    } else {
        let candidate = parent.join(relative).canonicalize()?;
        if !candidate.starts_with(&parent) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "virtual disk parent escapes child directory",
            ));
        }
        candidate
    };
    if !resolved.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "virtual disk parent is not a regular file",
        ));
    }
    Ok(resolved)
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
