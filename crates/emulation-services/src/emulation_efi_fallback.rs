//! Host-side UEFI fallback boot-path installation for emulation sessions.
//!
//! A fresh VMware VM has an empty NVRAM, so a GPT Linux disk whose ESP lacks
//! `\EFI\BOOT\BOOTX64.EFI` drops into the firmware boot manager instead of
//! starting the installed system. This service locates the vendor boot chain
//! on the ESP and installs it at the fallback path through the session's
//! copy-on-write overlay — the evidence image is never written, and the
//! change composes with the other host-side edits of the session.
//!
//! Loader strategy, in preference order:
//! 1. shim (`shimx64.efi`) together with its `grubx64.efi` (and `mmx64.efi`
//!    when present), copied side by side — Secure Boot-capable chains keep
//!    working because shim locates GRUB next to itself.
//! 2. a vendor `grubx64.efi` copied directly; distribution GRUB images carry
//!    a baked-in prefix, so loading them from the fallback path still finds
//!    the vendor configuration.
//! 3. `systemd-bootx64.efi` as the fallback loader.

use std::sync::Arc;

use evidence_core::volume::gpt::{
    classify_partition_type, parse_gpt_entries, parse_gpt_header, GptPartitionType,
};
use evidence_core::{EvidenceReader, FileSystemReader, PartitionWindowReader};
use evidence_emulation::CowDisk;
use transport::dto::{EmulationEfiFallbackResultDto, EmulationEfiFallbackStrategyDto};

use crate::emulation_bypass::EmulationBypassError;
use crate::emulation_cow_reader::CowDiskReader;

/// One loader binary is a few hundred KiB; anything beyond this bound means
/// the enumeration is looking at something unexpected.
const MAX_LOADER_BYTES: u64 = 16 * 1024 * 1024;
/// Mirrors the firmware-detection bounds: real GPT layouts hold a handful of
/// entries.
const MAX_GPT_ENTRY_COUNT: u32 = 4096;
const MAX_GPT_ENTRY_SIZE: u32 = 4096;

struct CowFatIo {
    disk: Arc<CowDisk>,
    partition_offset: u64,
    partition_length: u64,
}

impl CowFatIo {
    fn absolute(&self, offset: u64, length: usize) -> std::io::Result<u64> {
        // A corrupt FAT layout may compute offsets past the ESP; writes must
        // stay inside the partition window.
        offset
            .checked_add(length as u64)
            .filter(|end| *end <= self.partition_length)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "FAT write escapes the EFI system partition",
                )
            })?;
        self.partition_offset
            .checked_add(offset)
            .ok_or_else(|| std::io::Error::other("ESP offset overflow"))
    }
}

impl fs_fat::FatBlockIo for CowFatIo {
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> std::io::Result<()> {
        let absolute = self.absolute(offset, buffer.len())?;
        self.disk
            .read_exact_at(absolute, buffer)
            .map_err(std::io::Error::other)
    }

    fn write_at(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        let absolute = self.absolute(offset, data.len())?;
        self.disk
            .write_all_at(absolute, data)
            .map_err(std::io::Error::other)
    }
}

struct EspLocation {
    gpt_index: u32,
    offset: u64,
    length: u64,
}

fn locate_esp(disk: &Arc<CowDisk>) -> Result<EspLocation, EmulationBypassError> {
    if disk.len() < 1024 {
        return Err(EmulationBypassError::Unsupported(
            "the data source is too small to be GPT-partitioned".to_string(),
        ));
    }
    let mut header = [0u8; 512];
    disk.read_exact_at(512, &mut header)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let Some(header) = parse_gpt_header(&header) else {
        return Err(EmulationBypassError::Unsupported(
            "the data source is not GPT-partitioned; legacy boot needs no EFI fallback".to_string(),
        ));
    };
    let count = header.partition_count.min(MAX_GPT_ENTRY_COUNT);
    let entry_size = header.entry_size.clamp(128, MAX_GPT_ENTRY_SIZE);
    let byte_len = count as usize * entry_size as usize;
    let Some(offset) = header
        .partition_entry_lba
        .checked_mul(512)
        .filter(|offset| offset + byte_len as u64 <= disk.len())
    else {
        return Err(EmulationBypassError::Unsupported(
            "the GPT entry array lies outside the disk".to_string(),
        ));
    };
    let mut entries = vec![0u8; byte_len];
    disk.read_exact_at(offset, &mut entries)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    parse_gpt_entries(&entries, entry_size, count)
        .iter()
        .find(|partition| {
            classify_partition_type(&partition.type_guid) == GptPartitionType::EfiSystem
        })
        .and_then(|partition| {
            let offset = partition.start_lba.checked_mul(512)?;
            let length = partition
                .end_lba
                .checked_sub(partition.start_lba)?
                .checked_add(1)?
                .checked_mul(512)?;
            (offset + length <= disk.len()).then_some(EspLocation {
                gpt_index: partition.index as u32,
                offset,
                length,
            })
        })
        .ok_or_else(|| {
            EmulationBypassError::Unsupported(
                "no EFI system partition found on the data source".to_string(),
            )
        })
}

fn open_esp(
    disk: &Arc<CowDisk>,
    esp: &EspLocation,
) -> Result<fs_fat::FatReader, EmulationBypassError> {
    let window = PartitionWindowReader::new(
        Box::new(CowDiskReader::new(Arc::clone(disk))) as Box<dyn EvidenceReader>,
        esp.offset,
        Some(esp.length),
    )
    .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    fs_fat::FatReader::open(Box::new(window), 0).map_err(|error| {
        EmulationBypassError::Unsupported(format!("ESP is not a readable FAT volume: {error}"))
    })
}

fn read_loader(fs: &fs_fat::FatReader, path: &str) -> Result<Vec<u8>, EmulationBypassError> {
    fs.read_file_range(path, 0, MAX_LOADER_BYTES as usize)
        .map_err(|error| EmulationBypassError::EvidenceRead(format!("{path}: {error}")))
}

/// The vendor boot chain found on the ESP, as `(target name, source path)`
/// pairs plus the strategy they imply.
struct BootChain {
    strategy: EmulationEfiFallbackStrategyDto,
    files: Vec<(String, String)>,
}

fn find_boot_chain(fs: &fs_fat::FatReader) -> Result<BootChain, EmulationBypassError> {
    let efi = fs
        .list_children("EFI")
        .map_err(|error| EmulationBypassError::EvidenceRead(format!("EFI: {error}")))?;
    let has_file = |dir: &str, name: &str| {
        fs.list_children(dir)
            .map(|children| {
                children
                    .iter()
                    .find(|node| !node.is_dir && node.name.eq_ignore_ascii_case(name))
                    .map(|node| node.name.clone())
            })
            .unwrap_or(None)
    };
    for vendor in efi.iter().filter(|node| node.is_dir) {
        if vendor.name.eq_ignore_ascii_case("BOOT") {
            continue;
        }
        let dir = format!("EFI/{}", vendor.name);
        let shim = has_file(&dir, "shimx64.efi");
        let grub = has_file(&dir, "grubx64.efi");
        if let (Some(shim), Some(grub)) = (shim, grub.clone()) {
            let mut files = vec![
                ("BOOTX64.EFI".to_string(), format!("{dir}/{shim}")),
                ("GRUBX64.EFI".to_string(), format!("{dir}/{grub}")),
            ];
            if let Some(mm) = has_file(&dir, "mmx64.efi") {
                files.push(("MMX64.EFI".to_string(), format!("{dir}/{mm}")));
            }
            return Ok(BootChain {
                strategy: EmulationEfiFallbackStrategyDto::Shim,
                files,
            });
        }
        if let Some(grub) = grub {
            return Ok(BootChain {
                strategy: EmulationEfiFallbackStrategyDto::Grub,
                files: vec![("BOOTX64.EFI".to_string(), format!("{dir}/{grub}"))],
            });
        }
    }
    if let Some(loader) = has_file("EFI/systemd", "systemd-bootx64.efi") {
        return Ok(BootChain {
            strategy: EmulationEfiFallbackStrategyDto::SystemdBoot,
            files: vec![("BOOTX64.EFI".to_string(), format!("EFI/systemd/{loader}"))],
        });
    }
    Err(EmulationBypassError::Unsupported(
        "no shim, GRUB or systemd-boot loader found on the ESP".to_string(),
    ))
}

/// Install the UEFI fallback boot path into the session disk. Returns
/// `already_present` when `\EFI\BOOT\BOOTX64.EFI` exists, leaving the ESP
/// untouched.
pub fn install_efi_fallback(
    disk: &Arc<CowDisk>,
    data_source_id: &str,
) -> Result<EmulationEfiFallbackResultDto, EmulationBypassError> {
    let esp = locate_esp(disk)?;
    let base = EmulationEfiFallbackResultDto {
        session_id: String::new(),
        data_source_id: data_source_id.to_string(),
        esp_partition_index: esp.gpt_index,
        strategy: None,
        files_written: Vec::new(),
        already_present: false,
    };
    {
        let fs = open_esp(disk, &esp)?;
        if fs.open_file("EFI/BOOT/BOOTX64.EFI").is_ok() {
            return Ok(EmulationEfiFallbackResultDto {
                already_present: true,
                ..base
            });
        }
        let chain = find_boot_chain(&fs)?;
        let files: Vec<(String, Vec<u8>)> = chain
            .files
            .iter()
            .map(|(target, source)| read_loader(&fs, source).map(|data| (target.clone(), data)))
            .collect::<Result<_, _>>()?;
        let io = CowFatIo {
            disk: Arc::clone(disk),
            partition_offset: esp.offset,
            partition_length: esp.length,
        };
        let window = PartitionWindowReader::new(
            Box::new(CowDiskReader::new(Arc::clone(disk))) as Box<dyn EvidenceReader>,
            esp.offset,
            Some(esp.length),
        )
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
        let install = fs_fat::install_efi_fallback(Box::new(window), 0, &io, &files)
            .map_err(|error| EmulationBypassError::EspEdit(error.to_string()))?;
        // Semantic verification: a fresh filesystem view over the overlay
        // must resolve the fallback loader with the exact bytes written.
        let verify = open_esp(disk, &esp)?;
        let written = read_loader(&verify, "EFI/BOOT/BOOTX64.EFI")?;
        if written != files[0].1 {
            return Err(EmulationBypassError::EspEdit(
                "fallback loader verification mismatch after overlay write".to_string(),
            ));
        }
        Ok(EmulationEfiFallbackResultDto {
            strategy: Some(chain.strategy),
            files_written: install.files_written,
            ..base
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/emulation_efi_fallback.rs"]
mod tests;
