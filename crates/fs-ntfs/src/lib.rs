//! NTFS filesystem reader.
//! Parses boot sector to locate $MFT, reads FILE records, enumerates file names.
//! Full attribute parsing ($DATA, $INDEX_ROOT, INDX) is future work.

use evidence_core::filesystem::{FileSystemReader, FsNode};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

#[allow(dead_code)]
pub struct NtfsReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    mft_cluster: u64,
    mft_record_size: u32,
    cluster_size: u64,
}

impl NtfsReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut boot = [0u8; 512];
        reader.read_exact(&mut boot)?;

        if &boot[3..11] != b"NTFS    " {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a valid NTFS volume",
            ));
        }

        let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
        let sectors_per_cluster = boot[13];
        if bytes_per_sector == 0 || sectors_per_cluster == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid NTFS geometry",
            ));
        }
        let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
        let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap());
        let _root_dir = root_dir_frn(&boot);
        let mft_record_size = mft_record_bytes(&boot);

        Ok(Self {
            reader: RefCell::new(reader),
            bytes_per_sector,
            sectors_per_cluster,
            mft_cluster,
            mft_record_size,
            cluster_size,
        })
    }

    fn mft_offset(&self, record_number: u64) -> u64 {
        self.mft_cluster * self.cluster_size + record_number * self.mft_record_size as u64
    }

    fn read_mft_record(&self, record_number: u64) -> io::Result<Vec<u8>> {
        let off = self.mft_offset(record_number);
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(off))?;
        let mut rec = vec![0u8; self.mft_record_size as usize];
        reader.read_exact(&mut rec)?;
        Ok(rec)
    }

    fn parse_index_root(record: &[u8]) -> Vec<FsNode> {
        let mut nodes = Vec::new();
        if record.len() < 0x18 || &record[0..4] != b"FILE" {
            return nodes;
        }
        let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
        let mut pos = attr_off;
        while pos + 8 < record.len() {
            let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().unwrap());
            if typ == 0xFFFFFFFF {
                break;
            }
            let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().unwrap()) as usize;
            if len == 0 || pos + len > record.len() {
                break;
            }
            if typ == 0x90 && pos + 0x18 <= record.len() {
                let entries_off =
                    u32::from_le_bytes(record[pos + 0x10..pos + 0x14].try_into().unwrap()) as usize;
                let ents_start = pos + 0x10 + entries_off;
                if ents_start < pos + len {
                    nodes = parse_indx_entries(&record[ents_start..pos + len]);
                }
            }
            pos += len;
        }
        nodes
    }

    pub fn list_root_children(&self) -> io::Result<Vec<FsNode>> {
        let rec = self.read_mft_record(5)?;
        Ok(Self::parse_index_root(&rec))
    }

    fn find_dir_record(&self, name: &str) -> io::Result<Option<u64>> {
        let name_lower = name.to_lowercase();
        // Scan first 1024 MFT records looking for matching directory
        for rec_num in 0..1024u64 {
            let rec = match self.read_mft_record(rec_num) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if rec.len() < 4 || &rec[0..4] != b"FILE" { continue; }
            // Check record flags: bit 0x0002 = directory
            let flags = u16::from_le_bytes([rec[0x16], rec[0x17]]);
            if flags & 0x0002 == 0 { continue; } // not a directory

            // Look for $FILE_NAME (0x30) attribute with matching name
            let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
            let mut pos = attr_off;
            while pos + 8 < rec.len() {
                let typ = u32::from_le_bytes(rec[pos..pos+4].try_into().unwrap_or([0;4]));
                if typ == 0xFFFFFFFF { break; }
                let len = u32::from_le_bytes(rec[pos+4..pos+8].try_into().unwrap_or([0;4])) as usize;
                if len == 0 || pos + len > rec.len() { break; }
                if typ == 0x30 && pos + 0x5A < rec.len() {
                    let name_chars = rec[pos + 0x40] as usize;
                    if name_chars > 0 && pos + 0x5A + name_chars * 2 <= rec.len() {
                        let name_bytes = &rec[pos + 0x5A..pos + 0x5A + name_chars * 2];
                        let chars: Vec<u16> = name_bytes.chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                        let fname = String::from_utf16_lossy(&chars);
                        if fname.to_lowercase() == name_lower {
                            return Ok(Some(rec_num));
                        }
                    }
                }
                pos += len;
            }
        }
        Ok(None)
    }

    pub fn list_subdir_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        // Extract the last component of the path as the directory name
        let dir_name = path.rsplit('\\').next().unwrap_or(path);
        if dir_name.is_empty() {
            return self.list_root_children();
        }
        if let Some(rec_num) = self.find_dir_record(dir_name)? {
            let rec = self.read_mft_record(rec_num)?;
            Ok(Self::parse_index_root(&rec))
        } else {
            Ok(Vec::new())
        }
    }
}

impl FileSystemReader for NtfsReader {
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
            return self.list_root_children();
        }
        self.list_subdir_children(path)
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "NTFS file read not yet implemented",
        ))
    }

    fn data_source_name(&self) -> &str {
        "NTFS"
    }
}

fn parse_indx_entries(data: &[u8]) -> Vec<FsNode> {
    let mut nodes = Vec::new();
    let mut off = 0usize;
    while off + 0x52 < data.len() {
        let _mft_ref = u64::from_le_bytes(data[off..off + 8].try_into().unwrap_or([0; 8]));
        let entry_size = u16::from_le_bytes([data[off + 8], data[off + 9]]) as usize;
        if entry_size < 0x52 || off + entry_size > data.len() {
            break;
        }
        let name_len = data[off + 0x50] as usize;
        let name_start = off + 0x52;
        if name_len > 0
            && name_start + name_len * 2 <= data.len()
            && name_start + name_len * 2 <= off + entry_size
        {
            let chars: Vec<u16> = data[name_start..name_start + name_len * 2]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let name = String::from_utf16_lossy(&chars);
            let flags = if off + 0x4C < data.len() {
                u32::from_le_bytes(data[off + 0x48..off + 0x4C].try_into().unwrap_or([0; 4]))
            } else {
                0
            };
            let is_dir = flags & 0x10000000 != 0;
            nodes.push(FsNode {
                name,
                path: String::new(),
                is_dir,
                size: 0,
                created_at: None,
                modified_at: None,
                accessed_at: None,
            });
        }
        off += entry_size;
    }
    nodes
}

// --- Boot sector parsing helpers ---

fn root_dir_frn(boot: &[u8]) -> u64 {
    let mft_ref = u64::from_le_bytes(boot[0x2C..0x34].try_into().unwrap());
    mft_ref & 0x0000_FFFF_FFFF_FFFF
}

fn mft_record_bytes(boot: &[u8]) -> u32 {
    let raw = i32::from_le_bytes(boot[0x40..0x44].try_into().unwrap());
    if raw > 0 {
        1024
    } else if raw < 0 && (-raw) < 32 {
        (1u32 << (-raw as u32)).max(512)
    } else {
        1024
    }
}
