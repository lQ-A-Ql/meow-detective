use crate::dir::DirectoryEntry;
use crate::reader::ExfatReader;
use evidence_core::filesystem::invalid_fs_data;
use std::io;

const UPCASE_CODEPOINTS: usize = 0x1_0000;
const MAX_UPCASE_TABLE_BYTES: u64 = 512 * 1024;

/// exFAT's UTF-16 code-unit case mapping table.
#[derive(Debug, Clone)]
pub(crate) struct UpcaseTable {
    mappings: Vec<u16>,
}

impl UpcaseTable {
    pub(crate) fn fallback() -> Self {
        let mut mappings = vec![0u16; UPCASE_CODEPOINTS];
        for code in b'a'..=b'z' {
            mappings[usize::from(code)] = u16::from(code - b'a' + b'A');
        }
        Self { mappings }
    }

    pub(crate) fn fold(&self, value: &str) -> Vec<u16> {
        value
            .encode_utf16()
            .map(|code_unit| {
                let mapped = self.mappings[usize::from(code_unit)];
                if mapped == 0 {
                    code_unit
                } else {
                    mapped
                }
            })
            .collect()
    }

    fn from_compressed(data: &[u8]) -> io::Result<Self> {
        if data.is_empty() || !data.len().is_multiple_of(2) {
            return Err(invalid_fs_data("exFAT up-case table has invalid length"));
        }

        let mut mappings = vec![0u16; UPCASE_CODEPOINTS];
        let mut index = 0usize;
        let mut skip = false;
        for raw in data.chunks_exact(2) {
            if index >= UPCASE_CODEPOINTS {
                break;
            }
            let value = u16::from_le_bytes([raw[0], raw[1]]);
            if skip {
                index = index
                    .checked_add(usize::from(value))
                    .ok_or_else(|| invalid_fs_data("exFAT up-case table index overflows"))?;
                skip = false;
            } else if usize::from(value) == index {
                index += 1;
            } else if value == u16::MAX {
                skip = true;
            } else {
                mappings[index] = value;
                index += 1;
            }
        }

        if skip || index < 0xFFFF {
            return Err(invalid_fs_data(format!(
                "exFAT up-case table does not cover all UTF-16 code units (index {index:#x})"
            )));
        }
        Ok(Self { mappings })
    }

    fn from_directory_entry(reader: &ExfatReader, entry: (u32, u64, u32)) -> io::Result<Self> {
        let (first_cluster, data_length, expected_checksum) = entry;
        if data_length == 0 {
            return Err(invalid_fs_data("exFAT up-case table is empty"));
        }
        if data_length > MAX_UPCASE_TABLE_BYTES {
            return Err(invalid_fs_data(format!(
                "exFAT up-case table exceeds {} KiB",
                MAX_UPCASE_TABLE_BYTES / 1024
            )));
        }
        let data = reader.read_entry_data(first_cluster, data_length, false)?;
        let actual_checksum = checksum32(&data);
        if actual_checksum != expected_checksum {
            return Err(invalid_fs_data(format!(
                "exFAT up-case table checksum mismatch: expected {expected_checksum:#010x}, got {actual_checksum:#010x}"
            )));
        }
        Self::from_compressed(&data)
    }
}

pub(crate) fn load(reader: &ExfatReader) -> io::Result<UpcaseTable> {
    let root_data = reader.read_cluster_chain_data(reader.boot.first_cluster_of_root)?;
    let mut upcase_entry = None;
    for chunk in root_data.chunks_exact(crate::types::DIR_ENTRY_SIZE) {
        match DirectoryEntry::parse(chunk)? {
            DirectoryEntry::UpcaseTable {
                checksum,
                first_cluster,
                data_length,
            } => {
                if upcase_entry.is_some() {
                    return Err(invalid_fs_data(
                        "exFAT root directory contains multiple up-case tables",
                    ));
                }
                upcase_entry = Some((first_cluster, data_length, checksum));
            }
            DirectoryEntry::Deleted => {}
            DirectoryEntry::Unknown(_)
            | DirectoryEntry::File { .. }
            | DirectoryEntry::Stream { .. }
            | DirectoryEntry::FileName { .. }
            | DirectoryEntry::Bitmap { .. }
            | DirectoryEntry::VolumeLabel { .. } => {}
        }
    }

    match upcase_entry {
        Some(entry) => UpcaseTable::from_directory_entry(reader, entry),
        None => Ok(UpcaseTable::fallback()),
    }
}

fn checksum32(data: &[u8]) -> u32 {
    data.iter().fold(0u32, |checksum, byte| {
        checksum.rotate_right(1).wrapping_add(u32::from(*byte))
    })
}

#[cfg(test)]
#[path = "../tests/unit/upcase.rs"]
mod tests;
