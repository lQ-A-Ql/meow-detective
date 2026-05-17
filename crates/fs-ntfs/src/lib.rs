//! NTFS filesystem reader.
//! Parses boot sector to locate $MFT, reads FILE records, enumerates file names.
//! Full attribute parsing ($DATA, $INDEX_ROOT, INDX) is future work.

use evidence_core::filesystem::{FileSystemReader, FsNode};
use evidence_core::EvidenceReader;
use std::io::{self, Read, Seek, SeekFrom};

#[allow(dead_code)]
pub struct NtfsReader {
    reader: Box<dyn EvidenceReader>,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    mft_cluster: u64,
    mft_record_size: u32,
    cluster_size: u64,
}

impl NtfsReader {
    /// Open NTFS from an EvidenceReader at the given partition offset
    #[allow(dead_code)]
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
            reader,
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

    #[allow(dead_code)]
    fn read_file_record(&mut self, record_number: u64) -> io::Result<Vec<u8>> {
        let off = self.mft_offset(record_number);
        self.reader.seek(SeekFrom::Start(off))?;
        let mut rec = vec![0u8; self.mft_record_size as usize];
        self.reader.read_exact(&mut rec)?;
        Ok(rec)
    }

    #[allow(dead_code)]
    fn parse_file_names(record: &[u8]) -> Vec<String> {
        if record.len() < 4 || &record[0..4] != b"FILE" {
            return vec![];
        }
        let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
        let mut pos = attr_off;
        let mut names = Vec::new();
        while pos + 8 < record.len() {
            let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().unwrap());
            if typ == 0xFFFFFFFF {
                break;
            }
            let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().unwrap()) as usize;
            if len == 0 || pos + len > record.len() {
                break;
            }
            if typ == 0x30 && pos + 0x5A < record.len() {
                let name_chars = record[pos + 0x40] as usize;
                if name_chars > 0 && pos + 0x5A + name_chars * 2 <= record.len() {
                    let name_bytes = &record[pos + 0x5A..pos + 0x5A + name_chars * 2];
                    let chars: Vec<u16> = name_bytes
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    names.push(String::from_utf16_lossy(&chars));
                }
            }
            pos += len;
        }
        names
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

    fn list_children(&self, _path: &str) -> io::Result<Vec<FsNode>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "NTFS directory indexing not yet implemented",
        ))
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "NTFS file reading not yet implemented",
        ))
    }

    fn data_source_name(&self) -> &str {
        "NTFS"
    }
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
