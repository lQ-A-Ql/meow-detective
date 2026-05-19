//! NTFS filesystem reader.
//! Parses boot sector to locate $MFT, reads FILE records, enumerates file names.
//! Supports resident and non-resident attributes via data run parsing.

use evidence_core::filesystem::{FileSystemReader, FsNode};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::collections::HashSet;
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
    /// Absolute offset of NTFS volume start in evidence.
    volume_offset: u64,
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
        let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));
        let _root_dir = root_dir_frn(&boot);
        let mft_record_size = mft_record_bytes(&boot);

        Ok(Self {
            reader: RefCell::new(reader),
            bytes_per_sector,
            sectors_per_cluster,
            mft_cluster,
            mft_record_size,
            cluster_size,
            volume_offset: offset,
        })
    }

    fn mft_offset(&self, record_number: u64) -> u64 {
        self.volume_offset + self.mft_cluster * self.cluster_size
            + record_number * self.mft_record_size as u64
    }

    /// Convert volume-relative cluster number to absolute evidence offset.
    fn cluster_to_offset(&self, lcn: i64) -> io::Result<u64> {
        if lcn < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("negative LCN {} in data run", lcn),
            ));
        }
        Ok(self.volume_offset + lcn as u64 * self.cluster_size)
    }

    fn read_mft_record(&self, record_number: u64) -> io::Result<Vec<u8>> {
        let off = self.mft_offset(record_number);
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(off))?;
        let mut rec = vec![0u8; self.mft_record_size as usize];
        reader.read_exact(&mut rec)?;
        Ok(rec)
    }

    /// List children of any directory by MFT inode number.
    /// Reads $INDEX_ROOT (resident) and falls back to $INDEX_ALLOCATION
    /// (non-resident B-Tree) for large directories.
    fn list_dir_by_inode(&self, inode: u64) -> io::Result<Vec<DirEntry>> {
        let rec = self.read_mft_record(inode)?;
        let mut entries = Vec::new();
        let mut seen = HashSet::new();

        // Walk attributes looking for $INDEX_ROOT (0x90) and $INDEX_ALLOCATION (0xA0)
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;
        let mut index_root_entries: Option<Vec<DirEntry>> = None;
        let mut index_alloc_entries: Option<Vec<DirEntry>> = None;

        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFFFFFF {
                break;
            }
            let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }

            if typ == 0x90 && pos + 0x18 <= rec.len() {
                // $INDEX_ROOT — resident entries
                let entries_off =
                    u32::from_le_bytes(rec[pos + 0x10..pos + 0x14].try_into().unwrap_or([0; 4])) as usize;
                let ents_start = pos + 0x10 + entries_off;
                if ents_start < pos + len {
                    index_root_entries = Some(parse_indx_entries(&rec[ents_start..pos + len]));
                }
            }

            if typ == 0xA0 && pos + 0x40 <= rec.len() {
                // $INDEX_ALLOCATION — non-resident B-Tree INDX records
                if let Ok(data) = self.read_attr_nonresident(pos, &rec) {
                    index_alloc_entries = Some(Self::parse_indx_buffer(&data));
                }
            }

            pos += len;
        }

        // Merge: $INDEX_ALLOCATION entries first (more complete), then
        // fill gaps from $INDEX_ROOT. Deduplicate by mft_ref.
        if let Some(alloc) = index_alloc_entries {
            for e in alloc {
                seen.insert(e.mft_ref);
                entries.push(e);
            }
        }
        if let Some(root) = index_root_entries {
            for e in root {
                if seen.insert(e.mft_ref) {
                    entries.push(e);
                }
            }
        }

        Ok(entries)
    }

    // --- Non-resident attribute & INDX record helpers ---

    /// Parse NTFS data run list. Returns Vec<(LCN, cluster_count)>.
    fn parse_data_runs(&self, mut data: &[u8]) -> io::Result<Vec<(i64, u64)>> {
        let mut runs = Vec::new();
        let mut prev_lcn: i64 = 0;
        while !data.is_empty() && data[0] != 0 {
            let header = data[0];
            let size_bytes = (header & 0x0F) as usize;
            let offset_bytes = ((header >> 4) & 0x0F) as usize;
            if size_bytes > 8 || offset_bytes > 8 {
                break; // invalid data run header nibbles
            }
            data = &data[1..];
            if data.len() < size_bytes + offset_bytes {
                break;
            }
            let cluster_count = read_sized_le(&data[..size_bytes]);
            data = &data[size_bytes..];
            let lcn_offset = read_sized_le_signed(&data[..offset_bytes]);
            data = &data[offset_bytes..];
            let lcn = if runs.is_empty() {
                lcn_offset
            } else {
                prev_lcn + lcn_offset
            };
            prev_lcn = lcn;
            if cluster_count == 0 {
                continue; // sparse run: LCN gap without data
            }
            runs.push((lcn, cluster_count));
        }
        Ok(runs)
    }

    /// Read non-resident attribute data by walking its data run list.
    fn read_attr_nonresident(&self, attr_pos: usize, record: &[u8]) -> io::Result<Vec<u8>> {
        // Verify non-resident flag
        if attr_pos + 9 > record.len() || (record[attr_pos + 8] & 1) == 0 {
            return Ok(Vec::new());
        }
        // data_run_offset is at +0x20 in the non-resident header
        let run_off = u16::from_le_bytes([
            record[attr_pos + 0x20],
            record[attr_pos + 0x21],
        ]) as usize;
        // allocated size at +0x28
        let alloc_size = u64::from_le_bytes(
            record[attr_pos + 0x28..attr_pos + 0x30].try_into().unwrap_or([0; 8]),
        );
        if run_off == 0 || alloc_size == 0 || attr_pos + run_off >= record.len() {
            return Ok(Vec::new());
        }

        // Upper bound to avoid OOM on corrupt data
        if alloc_size > 128 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("attribute allocation too large: {} bytes", alloc_size),
            ));
        }

        let runs = self.parse_data_runs(&record[attr_pos + run_off..])?;
        let mut buf = Vec::with_capacity(alloc_size as usize);
        let mut reader = self.reader.borrow_mut();

        for (lcn, count) in runs {
            let offset = self.cluster_to_offset(lcn)?;
            let chunk = count * self.cluster_size;
            reader.seek(SeekFrom::Start(offset))?;
            let start = buf.len();
            buf.resize(start + chunk as usize, 0);
            reader.read_exact(&mut buf[start..])?;
        }

        // Trim to actual data (last cluster may be partial)
        let real_size = u64::from_le_bytes(
            record[attr_pos + 0x30..attr_pos + 0x38].try_into().unwrap_or([0; 8]),
        );
        if (real_size as usize) < buf.len() {
            buf.truncate(real_size as usize);
        }
        Ok(buf)
    }

    /// Scan INDX buffer for INDX records, apply fixup, extract entries.
    fn parse_indx_buffer(data: &[u8]) -> Vec<DirEntry> {
        let mut entries = Vec::new();
        let mut off = 0usize;
        while off + 0x18 < data.len() {
            let magic = u32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4]));
            // INDX record magic: "INDX" = 0x58444E49 (little-endian)
            if magic != 0x58444E49 {
                off += 1;
                continue;
            }
            let upd_off = u16::from_le_bytes([data[off + 4], data[off + 5]]) as usize;
            let upd_cnt = u16::from_le_bytes([data[off + 6], data[off + 7]]) as usize;
            if !(2..=64).contains(&upd_cnt) {
                off += 4;
                continue;
            }

            // Apply update sequence fixup
            // Copy only the sectors we need for fixup
            let copy_len = (upd_cnt * 512).min(data.len() - off);
            let mut rec = data[off..off + copy_len].to_vec();
            if upd_off + upd_cnt * 2 <= rec.len() {
                let orig = u16::from_le_bytes([rec[upd_off], rec[upd_off + 1]]);
                for i in 1..upd_cnt {
                    let fix_off = i * 512 - 2;
                    if fix_off + 2 > rec.len() {
                        break;
                    }
                    let val = u16::from_le_bytes([rec[fix_off], rec[fix_off + 1]]);
                    if val == orig {
                        let repl_off = upd_off + 2 + (i - 1) * 2;
                        if repl_off + 2 <= rec.len() {
                            rec[fix_off] = rec[repl_off];
                            rec[fix_off + 1] = rec[repl_off + 1];
                        }
                    }
                }
            }

            // Parse entries from the fixed-up record
            // Index entry list starts at +0x18
            let list_start = 0x18usize;
            if list_start + 4 <= rec.len() {
                let ent_off = u32::from_le_bytes(
                    rec[list_start..list_start + 4].try_into().unwrap_or([0; 4]),
                ) as usize;
                let ent_total = u32::from_le_bytes(
                    rec[list_start + 4..list_start + 8].try_into().unwrap_or([0; 4]),
                ) as usize;
                let idxe_start = list_start + ent_off;
                let idxe_end = (list_start + ent_off + ent_total).min(rec.len());
                if idxe_start < idxe_end {
                    let mut indx_entries =
                        parse_indx_entries(&rec[idxe_start..idxe_end]);
                    entries.append(&mut indx_entries);
                }
            }

            // Move ahead one sector (512 bytes) after processing a valid INDX record.
            off += 512;
        }
        entries
    }

    pub fn list_root_children(&self) -> io::Result<Vec<FsNode>> {
        Ok(self.list_dir_by_inode(5)?.into_iter().map(|e| e.node).collect())
    }

    /// Read the $DATA attribute of a file by MFT inode.
    /// Handles both resident (inline) and non-resident (data run chain) $DATA.
    fn read_file_data(&self, inode: u64) -> io::Result<Vec<u8>> {
        let rec = self.read_mft_record(inode)?;
        if rec.len() < 0x18 || &rec[0..4] != b"FILE" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("inode {} is not a valid FILE record", inode),
            ));
        }
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFFFFFF {
                break;
            }
            let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            if typ == 0x80 {
                let is_nonresident = pos + 9 <= rec.len() && (rec[pos + 8] & 1) != 0;
                if is_nonresident {
                    if pos + 0x40 > rec.len() {
                        return Ok(Vec::new());
                    }
                    return self.read_attr_nonresident(pos, &rec);
                } else {
                    if pos + 0x16 > rec.len() {
                        return Ok(Vec::new());
                    }
                    let content_size =
                        u32::from_le_bytes(rec[pos + 0x10..pos + 0x14].try_into().unwrap_or([0; 4]))
                            as usize;
                    let content_off = pos
                        + u16::from_le_bytes(rec[pos + 0x14..pos + 0x16].try_into().unwrap_or([0; 2]))
                            as usize;
                    let end = content_off.saturating_add(content_size).min(rec.len());
                    if content_off < end {
                        return Ok(rec[content_off..end].to_vec());
                    }
                    return Ok(Vec::new());
                }
            }
            pos += len;
        }
        Ok(Vec::new())
    }

    /// Resolve a file path: walk parent directories, then find the file
    /// in the final directory. Returns file MFT inode, or None if not found.
    fn resolve_file_path(&self, path: &str) -> io::Result<Option<u64>> {
        let components: Vec<&str> = path
            .trim_start_matches('\\')
            .split('\\')
            .filter(|c| !c.is_empty())
            .collect();
        let (parent_dirs, file_name) = match components.split_last() {
            Some((file, dirs)) => (dirs, *file),
            None => return Ok(None),
        };

        let mut current_inode = 5u64;
        for dir in parent_dirs {
            let children = self.list_dir_by_inode(current_inode)?;
            let found = children
                .iter()
                .find(|e| e.node.name.eq_ignore_ascii_case(dir) && e.node.is_dir);
            match found {
                Some(entry) => current_inode = entry.mft_ref,
                None => return Ok(None),
            }
        }

        let children = self.list_dir_by_inode(current_inode)?;
        Ok(children
            .iter()
            .find(|e| e.node.name.eq_ignore_ascii_case(file_name) && !e.node.is_dir)
            .map(|e| e.mft_ref))
    }

    /// Resolve a path from root, walking top-down through directory INDX entries.
    /// Returns the MFT inode of the final component, or None if not found.
    /// Validates $FILE_NAME.par_ref consistency at each step.
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
                    // Verify the child directory's $FILE_NAME points back to us
                    if !self.verify_parent(entry.mft_ref, current_inode)? {
                        return Ok(None);
                    }
                    current_inode = entry.mft_ref;
                    remaining = rest;
                }
                None => return Ok(None),
            }
        }
        Ok(Some(current_inode))
    }

    /// Verify that the $FILE_NAME attribute of `child_inode` has
    /// `par_ref` == `expected_parent`. Returns false on mismatch or IO error.
    fn verify_parent(&self, child_inode: u64, expected_parent: u64) -> io::Result<bool> {
        let rec = match self.read_mft_record(child_inode) {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(
                rec[pos..pos + 4].try_into().unwrap_or([0xFF, 0xFF, 0xFF, 0xFF]),
            );
            if typ == 0xFFFFFFFF {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            if typ == 0x30 && pos + 8 <= rec.len() {
                // $FILE_NAME: par_ref is a 48-bit value at attribute + 0x00
                let par_ref = u64::from_le_bytes(
                    rec[pos..pos + 8].try_into().unwrap_or([0; 8]),
                ) & 0x0000_FFFF_FFFF_FFFF;
                return Ok(par_ref == expected_parent);
            }
            pos += len;
        }
        Ok(true) // no $FILE_NAME found — can't verify, allow
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

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let inode = self
            .resolve_file_path(path)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not found"))?;
        let data = self.read_file_data(inode)?;
        // Guard against OOM on large files (128 MB limit)
        if data.len() > 128 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                format!("file too large to buffer: {} bytes", data.len()),
            ));
        }
        Ok(Box::new(io::Cursor::new(data)))
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
    let mft_ref = u64::from_le_bytes(boot[0x2C..0x34].try_into().unwrap_or([0; 8]));
    mft_ref & 0x0000_FFFF_FFFF_FFFF
}

fn mft_record_bytes(boot: &[u8]) -> u32 {
    let raw = i32::from_le_bytes(boot[0x40..0x44].try_into().unwrap_or([0; 4]));
    if raw > 0 {
        1024
    } else if raw < 0 && (-raw) < 32 {
        (1u32 << (-raw as u32)).max(512)
    } else {
        1024
    }
}

// --- Data run parsing helpers ---

/// Read a variable-width little-endian unsigned integer (1-8 bytes).
fn read_sized_le(bytes: &[u8]) -> u64 {
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate().take(8) {
        val |= (b as u64) << (i * 8);
    }
    val
}

/// Read a variable-width little-endian signed integer (1-8 bytes).
fn read_sized_le_signed(bytes: &[u8]) -> i64 {
    let n = bytes.len().min(8);
    if n == 0 {
        return 0;
    }
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate().take(n) {
        val |= (b as u64) << (i * 8);
    }
    // Sign-extend: if the highest bit of the last byte is set,
    // fill upper bytes with 0xFF.
    let last = bytes[n - 1];
    if last & 0x80 != 0 {
        for i in n..8 {
            val |= 0xFFu64 << (i * 8);
        }
    }
    val as i64
}
