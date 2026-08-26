//! Select a VMware storage controller from Linux initramfs contents.
//!
//! Linux distributions do not ship one uniform initramfs driver set.  The
//! controller rendered by VMware must therefore match the drivers present in
//! the guest's early userspace.  This module performs a bounded, read-only
//! inspection of initramfs cpio member names and never exposes host paths.

use std::io::{Cursor, Read};

use evidence_core::FileSystemReader;
use evidence_emulation::VmdkAdapter;

const MAX_INITRAMFS_BYTES: usize = 128 * 1024 * 1024;
const MAX_CPIO_ENTRIES: usize = 1_000_000;
const CPIO_HEADER_SIZE: usize = 110;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LinuxControllerEvidence {
    ide: bool,
    lsi: bool,
    found_initramfs: bool,
    decoded_initramfs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LinuxControllerDecision {
    pub(crate) adapter: VmdkAdapter,
    pub(crate) reason: &'static str,
}

impl LinuxControllerEvidence {
    pub(crate) fn merge(&mut self, other: Self) {
        self.ide |= other.ide;
        self.lsi |= other.lsi;
        self.found_initramfs |= other.found_initramfs;
        self.decoded_initramfs |= other.decoded_initramfs;
    }

    pub(crate) fn ide_is_decisive(self) -> bool {
        self.ide
    }

    pub(crate) fn decision(self) -> LinuxControllerDecision {
        if self.ide {
            return LinuxControllerDecision {
                adapter: VmdkAdapter::Ide,
                reason: "initramfs contains ata_piix; selected IDE",
            };
        }
        if self.lsi {
            return LinuxControllerDecision {
                adapter: VmdkAdapter::LsiLogic,
                reason: "initramfs contains mptspi; selected LsiLogic",
            };
        }
        if !self.found_initramfs {
            LinuxControllerDecision {
                adapter: VmdkAdapter::Ide,
                reason: "no initramfs was found; defaulted to IDE",
            }
        } else if !self.decoded_initramfs {
            LinuxControllerDecision {
                adapter: VmdkAdapter::Ide,
                reason: "initramfs could not be decoded; defaulted to IDE",
            }
        } else {
            LinuxControllerDecision {
                adapter: VmdkAdapter::Ide,
                reason: "no supported initramfs storage driver was found; defaulted to IDE",
            }
        }
    }
}

/// Inspect initramfs files beneath both `/boot` and the filesystem root.
/// A separate boot partition is represented by the latter form.
pub(crate) fn inspect_filesystem(fs: &dyn FileSystemReader) -> LinuxControllerEvidence {
    let mut paths = Vec::new();
    collect_candidates(fs, "boot", &mut paths);
    collect_candidates(fs, "", &mut paths);
    paths.sort_unstable();
    paths.dedup();

    let mut evidence = LinuxControllerEvidence::default();
    for path in paths {
        evidence.found_initramfs = true;
        let Ok(bytes) = fs.read_file_range(&path, 0, MAX_INITRAMFS_BYTES.saturating_add(1)) else {
            continue;
        };
        if bytes.len() > MAX_INITRAMFS_BYTES {
            continue;
        }
        let Some((ide, lsi)) = inspect_initramfs_driver_names(&bytes) else {
            continue;
        };
        evidence.decoded_initramfs = true;
        evidence.ide |= ide;
        evidence.lsi |= lsi;
        // IDE is intentionally the deterministic preference when any
        // initramfs in a multi-boot image contains ata_piix.
        if evidence.ide {
            break;
        }
    }
    evidence
}

fn collect_candidates(fs: &dyn FileSystemReader, directory: &str, paths: &mut Vec<String>) {
    let Ok(children) = fs.list_children(directory) else {
        return;
    };
    for child in children {
        if child.is_dir || !is_initramfs_name(&child.name) {
            continue;
        }
        let path = if directory.is_empty() {
            child.name
        } else {
            format!("{directory}/{}", child.name)
        };
        paths.push(path);
    }
}

fn is_initramfs_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "initramfs"
        || lower.starts_with("initramfs-")
        || lower.starts_with("initrd")
        || lower.contains("-initramfs")
}

fn read_bounded(reader: &mut dyn Read) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let limit = u64::try_from(MAX_INITRAMFS_BYTES.saturating_add(1)).ok()?;
    reader.take(limit).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= MAX_INITRAMFS_BYTES).then_some(bytes)
}

fn decode_compressed_initramfs(bytes: &[u8]) -> Option<Vec<u8>> {
    let decoded = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = flate2::read::MultiGzDecoder::new(Cursor::new(bytes));
        read_bounded(&mut decoder)?
    } else if bytes.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]) {
        let mut decoder = xz2::read::XzDecoder::new(Cursor::new(bytes));
        read_bounded(&mut decoder)?
    } else if bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(bytes)).ok()?;
        read_bounded(&mut decoder)?
    } else {
        return None;
    };
    is_cpio_magic(&decoded).then_some(decoded)
}

/// Linux initramfs images may prepend an uncompressed microcode cpio before
/// the compressed main archive. Walk each segment so that an early trailer
/// cannot hide the storage modules in the main archive.
fn inspect_initramfs_driver_names(bytes: &[u8]) -> Option<(bool, bool)> {
    let mut offset = 0usize;
    let mut found_archive = false;
    let mut ide = false;
    let mut lsi = false;
    while offset < bytes.len() {
        while offset < bytes.len() && bytes[offset] == 0 {
            offset += 1;
        }
        if offset == bytes.len() {
            break;
        }
        if is_cpio_magic(&bytes[offset..]) {
            let ((archive_ide, archive_lsi), consumed) = parse_cpio_archive(&bytes[offset..])?;
            ide |= archive_ide;
            lsi |= archive_lsi;
            found_archive = true;
            offset = offset.checked_add(consumed)?;
            continue;
        }
        let decoded = decode_compressed_initramfs(&bytes[offset..])?;
        let (archive_ide, archive_lsi) = inspect_initramfs_driver_names(&decoded)?;
        ide |= archive_ide;
        lsi |= archive_lsi;
        found_archive = true;
        break;
    }
    found_archive.then_some((ide, lsi))
}

fn is_cpio_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && (&bytes[..6] == b"070701" || &bytes[..6] == b"070702")
}

fn parse_cpio_archive(bytes: &[u8]) -> Option<((bool, bool), usize)> {
    let mut offset = 0usize;
    let mut ide = false;
    let mut lsi = false;
    for _ in 0..MAX_CPIO_ENTRIES {
        let end = offset.checked_add(CPIO_HEADER_SIZE)?;
        if end > bytes.len() || !is_cpio_magic(&bytes[offset..]) {
            return None;
        }
        let namesize = parse_hex_field(&bytes[offset + 94..offset + 102])?;
        let filesize = parse_hex_field(&bytes[offset + 54..offset + 62])?;
        let name_start = end;
        let name_end = name_start.checked_add(namesize)?;
        if namesize == 0 || name_end > bytes.len() || bytes[name_end - 1] != 0 {
            return None;
        }
        let name = &bytes[name_start..name_end.saturating_sub(1)];
        if name == b"TRAILER!!!" {
            let consumed = align4(name_end)?;
            return (filesize == 0 && consumed <= bytes.len()).then_some(((ide, lsi), consumed));
        }
        if is_kernel_module_entry(name, b"ata_piix.ko") {
            ide = true;
        }
        // VMware's LSI Logic Parallel adapter needs the mptspi transport.
        // mptbase/mptscsih are dependencies, while vmw_pvscsi belongs to a
        // different virtual controller and is not evidence for lsilogic.
        if is_kernel_module_entry(name, b"mptspi.ko") {
            lsi = true;
        }
        let data_start = align4(name_end)?;
        let data_end = data_start.checked_add(filesize)?;
        offset = align4(data_end)?;
        if offset > bytes.len() {
            return None;
        }
    }
    None
}

fn is_kernel_module_entry(path: &[u8], module: &[u8]) -> bool {
    let file_name = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    let (stem, suffix) = file_name
        .split_at_checked(module.len())
        .unwrap_or((&[], &[]));
    stem.eq_ignore_ascii_case(module)
        && (suffix.is_empty()
            || [b".gz".as_slice(), b".xz", b".zst", b".lz4"]
                .iter()
                .any(|expected| suffix.eq_ignore_ascii_case(expected)))
}

fn parse_hex_field(field: &[u8]) -> Option<usize> {
    if field.is_empty() {
        return None;
    }
    let mut value = 0usize;
    for byte in field {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return None,
        } as usize;
        value = value.checked_mul(16)?.checked_add(digit)?;
    }
    Some(value)
}

fn align4(value: usize) -> Option<usize> {
    value.checked_add(3).map(|aligned| aligned & !3)
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/emulation_linux_controller.rs"]
mod tests;
