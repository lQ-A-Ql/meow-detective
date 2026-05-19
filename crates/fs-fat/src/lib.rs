//! FAT12/16/32 filesystem reader.
//! Parses BPB, walks FAT cluster chains, reads directory entries.

use evidence_core::filesystem::{FileSystemReader, FsNode};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

pub struct FatReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fat_count: u8,
    #[allow(dead_code)]
    root_entries: u16,
    sectors_per_fat: u32,
    first_data_sector: u32,
    cluster_size: u64,
    fat_type: FatType,
    volume_offset: u64,
}

#[derive(Debug, PartialEq)]
enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

impl FatReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut boot = [0u8; 512];
        reader.read_exact(&mut boot)?;

        let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
        let sectors_per_cluster = boot[13];
        let reserved_sectors = u16::from_le_bytes([boot[14], boot[15]]);
        let fat_count = boot[16];
        let root_entries = u16::from_le_bytes([boot[17], boot[18]]);

        if bytes_per_sector == 0 || sectors_per_cluster == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid BPB"));
        }

        // FAT12/16 fields
        let fat16_sectors = u16::from_le_bytes([boot[22], boot[23]]) as u32;
        let sectors_per_fat: u32 = if fat16_sectors > 0 {
            fat16_sectors
        } else {
            u32::from_le_bytes(boot[36..40].try_into().unwrap_or([0; 4]))
        };

        let total16 = u16::from_le_bytes([boot[19], boot[20]]) as u32;
        let total_sectors: u32 = if total16 > 0 {
            total16
        } else {
            u32::from_le_bytes(boot[32..36].try_into().unwrap_or([0; 4]))
        };

        let root_dir_sectors = (root_entries as u32 * 32).div_ceil(bytes_per_sector as u32);

        let fat_size = fat_count as u32 * sectors_per_fat;
        let first_data_sector = reserved_sectors as u32 + fat_size + root_dir_sectors;

        let data_sectors = total_sectors.saturating_sub(first_data_sector);
        let cluster_count = data_sectors / sectors_per_cluster as u32;

        // FAT32 detection: check extended boot signature at offset 0x42
        let is_fat32 = boot.get(0x42) == Some(&0x28) || boot.get(0x42) == Some(&0x29);

        let fat_type = if is_fat32 {
            FatType::Fat32
        } else if cluster_count < 4085 {
            FatType::Fat12
        } else if cluster_count < 65525 {
            FatType::Fat16
        } else {
            FatType::Fat32
        };

        let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;

        Ok(Self {
            reader: RefCell::new(reader),
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            fat_count,
            root_entries,
            sectors_per_fat,
            first_data_sector,
            cluster_size,
            fat_type,
            volume_offset: offset,
        })
    }

    fn fat_offset(&self) -> u64 {
        self.volume_offset + self.reserved_sectors as u64 * self.bytes_per_sector as u64
    }

    fn cluster_to_offset(&self, cluster: u32) -> u64 {
        self.volume_offset
            + (self.first_data_sector as u64
                + (cluster as u64 - 2) * self.sectors_per_cluster as u64)
                * self.bytes_per_sector as u64
    }

    fn read_fat_entry(&self, cluster: u32) -> io::Result<u32> {
        let fat_off = self.fat_offset();
        let entry_offset: u64;
        let entry_size: usize;

        match self.fat_type {
            FatType::Fat12 => {
                entry_offset = fat_off + (cluster as u64 * 3 / 2);
                entry_size = 2;
            }
            FatType::Fat16 => {
                entry_offset = fat_off + cluster as u64 * 2;
                entry_size = 2;
            }
            FatType::Fat32 => {
                entry_offset = fat_off + cluster as u64 * 4;
                entry_size = 4;
            }
        }

        let mut buf = vec![0u8; entry_size];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(entry_offset))?;
        reader.read_exact(&mut buf)?;

        let raw = match entry_size {
            2 => u16::from_le_bytes([buf[0], buf[1]]) as u32,
            4 => u32::from_le_bytes(buf[0..4].try_into().unwrap_or([0; 4])),
            _ => 0,
        };

        if self.fat_type == FatType::Fat12 {
            if cluster & 1 != 0 {
                Ok(raw >> 4)
            } else {
                Ok(raw & 0x0FFF)
            }
        } else {
            Ok(raw & 0x0FFF_FFFF)
        }
    }

    fn is_eoc(&self, cluster: u32) -> bool {
        match self.fat_type {
            FatType::Fat12 => cluster >= 0x0FF8,
            FatType::Fat16 => cluster >= 0xFFF8,
            FatType::Fat32 => cluster >= 0x0FFF_FFF8,
        }
    }

    fn walk_cluster_chain(&self, start_cluster: u32) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut cluster = start_cluster;

        while cluster >= 2 && !self.is_eoc(cluster) {
            let offset = self.cluster_to_offset(cluster);
            let size = self.cluster_size as usize;
            let start = data.len();
            data.resize(start + size, 0);
            {
                let mut reader = self.reader.borrow_mut();
                reader.seek(SeekFrom::Start(offset))?;
                reader.read_exact(&mut data[start..])?;
            }
            cluster = self.read_fat_entry(cluster)?;
        }

        Ok(data)
    }

    fn parse_directory_entries(data: &[u8]) -> Vec<FsNode> {
        let mut nodes = Vec::new();
        let mut lfn_buf = String::new();
        let mut i = 0usize;

        while i + 32 <= data.len() {
            let entry = &data[i..i + 32];

            if entry[0] == 0x00 {
                break; // end of directory
            }
            if entry[0] == 0xE5 {
                i += 32;
                continue; // deleted entry
            }

            let attr = entry[11];
            // LFN entry
            if attr == 0x0F {
                let seq = entry[0] & 0x3F;
                let is_last = entry[0] & 0x40 != 0;
                if is_last {
                    lfn_buf.clear();
                }

                let mut name_part = String::new();
                // LFN name fragments (UTF-16LE)
                for chunk in &[
                    &entry[1..11],
                    &entry[14..26],
                    &entry[28..32],
                ] {
                    for pair in chunk.chunks(2) {
                        if pair.len() == 2 {
                            let ch = u16::from_le_bytes([pair[0], pair[1]]);
                            if ch != 0 && ch != 0xFFFF {
                                name_part.push(char::from_u32(ch as u32).unwrap_or('\u{FFFD}'));
                            }
                        }
                    }
                }

                let pos = (seq - 1) as usize * 13;
                while lfn_buf.len() < pos {
                    lfn_buf.push(' ');
                }
                if lfn_buf.len() == pos {
                    lfn_buf.push_str(&name_part);
                } else {
                    lfn_buf.insert_str(pos, &name_part);
                }

                i += 32;
                continue;
            }

            // SFN entry
            let name = if !lfn_buf.is_empty() {
                let n = lfn_buf.trim_end().to_string();
                lfn_buf.clear();
                n
            } else {
                read_sfn_name(entry)
            };

            if name.is_empty() || name == "." || name == ".." {
                i += 32;
                continue;
            }

            let is_dir = attr & 0x10 != 0;
            let size = u32::from_le_bytes(entry[28..32].try_into().unwrap_or([0; 4])) as u64;
            let _cluster =
                u16::from_le_bytes([entry[26], entry[27]]) as u32
                    | ((u16::from_le_bytes([entry[20], entry[21]]) as u32) << 16);

            nodes.push(FsNode {
                name,
                path: String::new(),
                is_dir,
                size,
                created_at: None,
                modified_at: None,
                accessed_at: None,
            });

            i += 32;
        }

        nodes
    }

    fn list_root(&self) -> io::Result<Vec<FsNode>> {
        if self.fat_type == FatType::Fat32 {
            // FAT32 root directory is in a cluster chain
            let root_cluster = self.first_data_sector.saturating_sub(2);
            let data = self.walk_cluster_chain(root_cluster)?;
            Ok(Self::parse_directory_entries(&data))
        } else {
            // FAT12/16 root directory follows the FAT tables
            let offset = self.volume_offset
                + (self.reserved_sectors as u64 + self.fat_count as u64 * self.sectors_per_fat as u64)
                    * self.bytes_per_sector as u64;
            let size = self.root_entries as usize * 32;
            let mut data = vec![0u8; size];
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(offset))?;
            reader.read_exact(&mut data)?;
            Ok(Self::parse_directory_entries(&data))
        }
    }

    /// Find a subdirectory cluster from parent directory entries.
    fn find_subdir_cluster(&self, parent_path: &str, name: &str) -> io::Result<Option<u32>> {
        let _entries = if parent_path.is_empty() {
            self.list_root()?
        } else {
            // For simplicity: list root and walk (single-level only for now)
            self.list_root()?
        };
        // Search entries for matching directory name — we need cluster numbers.
        // SFN entries have cluster at bytes 26-27 (low) and 20-21 (high).
        // Re-parse raw entries to get cluster numbers.
        let data = if parent_path.is_empty() {
            if self.fat_type == FatType::Fat32 {
                let root_cluster = self.first_data_sector.saturating_sub(2);
                self.walk_cluster_chain(root_cluster)?
            } else {
                let offset = self.volume_offset
                    + (self.reserved_sectors as u64
                        + self.fat_count as u64 * self.sectors_per_fat as u64)
                        * self.bytes_per_sector as u64;
                let size = self.root_entries as usize * 32;
                let mut buf = vec![0u8; size];
                let mut reader = self.reader.borrow_mut();
                reader.seek(SeekFrom::Start(offset))?;
                reader.read_exact(&mut buf)?;
                buf
            }
        } else {
            return Ok(None); // multi-level not supported yet
        };

        Ok(Self::find_entry_cluster(&data, name))
    }

    fn find_entry_cluster(data: &[u8], target: &str) -> Option<u32> {
        let target_lower = target.to_lowercase();
        let mut lfn_buf = String::new();
        let mut i = 0usize;

        while i + 32 <= data.len() {
            let entry = &data[i..i + 32];
            if entry[0] == 0x00 { break; }
            if entry[0] == 0xE5 { i += 32; continue; }

            let attr = entry[11];
            if attr == 0x0F {
                let is_last = entry[0] & 0x40 != 0;
                if is_last { lfn_buf.clear(); }
                let name = if !lfn_buf.is_empty() {
                    let n = lfn_buf.trim_end().to_string();
                    lfn_buf.clear();
                    n
                } else {
                    read_sfn_name(entry)
                };
                let cluster = u16::from_le_bytes([entry[26], entry[27]]) as u32
                    | ((u16::from_le_bytes([entry[20], entry[21]]) as u32) << 16);
                let is_dir = attr & 0x10 != 0;
                if name.to_lowercase() == target_lower && is_dir {
                    return Some(cluster);
                }
            }
            i += 32;
        }
        None
    }
}

impl FileSystemReader for FatReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(FsNode {
            name: "\\".into(),
            path: String::new(),
            is_dir: true,
            size: 0,
            created_at: None,
            modified_at: None,
            accessed_at: None,
        })
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if path.is_empty() {
            return self.list_root();
        }
        // Single-level subdirectory lookup
        let name = path.trim_start_matches('\\');
        if let Some(cluster) = self.find_subdir_cluster("", name)? {
            let data = self.walk_cluster_chain(cluster)?;
            return Ok(Self::parse_directory_entries(&data));
        }
        Ok(Vec::new())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "FAT file read not yet implemented",
        ))
    }

    fn data_source_name(&self) -> &str {
        match self.fat_type {
            FatType::Fat12 => "FAT12",
            FatType::Fat16 => "FAT16",
            FatType::Fat32 => "FAT32",
        }
    }
}

fn read_sfn_name(entry: &[u8]) -> String {
    let name = String::from_utf8_lossy(&entry[0..8]).trim_end().to_string();
    let ext = String::from_utf8_lossy(&entry[8..11]).trim_end().to_string();
    if ext.is_empty() {
        name
    } else {
        format!("{}.{}", name, ext)
    }
}
