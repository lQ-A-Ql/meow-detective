//! NTFS filesystem reader.
//! Parses boot sector to locate $MFT, reads FILE records, enumerates file names.
//! Full attribute parsing ($DATA, $INDEX_ROOT, INDX) is future work.

use evidence_core::filesystem::{FileSystemReader, FsNode};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

/// Internal entry with MFT reference for path resolution.
struct DirEntry {
    node: FsNode,
    mft_ref: u64,
}

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

    /// Parse $INDEX_ROOT attribute from an MFT record, returning children
    /// with their MFT references for path resolution.
    fn parse_index_root(record: &[u8]) -> Vec<DirEntry> {
        let mut entries = Vec::new();
        if record.len() < 0x18 || &record[0..4] != b"FILE" {
            return entries;
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
                    entries = parse_indx_entries(&record[ents_start..pos + len]);
                }
            }
            pos += len;
        }
        entries
    }

    /// List children of any directory by MFT inode number.
    fn list_dir_by_inode(&self, inode: u64) -> io::Result<Vec<DirEntry>> {
        let rec = self.read_mft_record(inode)?;
        Ok(Self::parse_index_root(&rec))
    }

    pub fn list_root_children(&self) -> io::Result<Vec<FsNode>> {
        Ok(self.list_dir_by_inode(5)?.into_iter().map(|e| e.node).collect())
    }

    /// Resolve a path from root, walking top-down through directory INDX entries.
    /// Returns the MFT inode of the final component, or None if not found.
    fn resolve_path(&self, path: &str) -> io::Result<Option<u64>> {
        let components: Vec<&str> = path
            .trim_start_matches('\\')
            .split('\\')
            .filter(|c| !c.is_empty())
            .collect();
        if components.is_empty() {
            return Ok(Some(5));
        }
        let mut current_inode = 5u64;
        let mut remaining = &components[..];
        while let Some((target, rest)) = remaining.split_first() {
            let children = self.list_dir_by_inode(current_inode)?;
            let found = children
                .iter()
                .find(|e| e.node.name.eq_ignore_ascii_case(target) && e.node.is_dir);
            match found {
                Some(entry) => {
                    current_inode = entry.mft_ref;
                    remaining = rest;
                }
                None => return Ok(None),
            }
        }
        Ok(Some(current_inode))
    }

    pub fn list_subdir_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        match self.resolve_path(path)? {
            Some(inode) => Ok(self
                .list_dir_by_inode(inode)?
                .into_iter()
                .map(|e| e.node)
                .collect()),
            None => Ok(Vec::new()),
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

/// Parse INDX entries from $INDEX_ROOT buffer. Returns DirEntry with
/// both the FsNode and the child MFT reference (lower 48 bits of file_ref).
fn parse_indx_entries(data: &[u8]) -> Vec<DirEntry> {
    let mut entries = Vec::new();
    let mut off = 0usize;
    while off + 0x52 < data.len() {
        let mft_ref = u64::from_le_bytes(data[off..off + 8].try_into().unwrap_or([0; 8]))
            & 0x0000_FFFF_FFFF_FFFF;
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
            entries.push(DirEntry {
                node: FsNode {
                    name,
                    path: String::new(),
                    is_dir,
                    size: 0,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                },
                mft_ref,
            });
        }
        off += entry_size;
    }
    entries
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
