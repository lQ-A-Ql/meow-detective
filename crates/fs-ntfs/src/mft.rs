//! NTFS MFT record reading, directory enumeration, and file system interface.

use crate::directory::{
    canonicalize_indx_entries, parse_indx_entries, DirEntry, NtfsDirectoryEntry,
};
use crate::utils::{
    index_record_bytes, mft_inode_from_path, mft_record_bytes, read_contiguous_mft_record,
    root_dir_frn,
};
use crate::{ATTR_TYPE_INDEX_ROOT, MAX_BUFFERED_FILE_BYTES};
use evidence_core::filesystem::{
    child_nodes_with_parent_path, invalid_fs_data as core_invalid_fs_data, root_node,
    FileSystemReader, FsNode,
};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

pub struct NtfsReader {
    pub(crate) reader: RefCell<Box<dyn EvidenceReader>>,
    pub(crate) bytes_per_sector: u16,
    /// Sectors per cluster (parsed for format completeness).
    pub(crate) _sectors_per_cluster: u8,
    pub(crate) mft_cluster: u64,
    pub(crate) mft_record_size: u32,
    pub(crate) index_record_size: u32,
    pub(crate) cluster_size: u64,
    pub(crate) mft_data_runs: Vec<(i64, u64)>,
    pub(crate) mft_record_count: u64,
    pub(crate) volume_serial: u64,
    /// Absolute offset of NTFS volume start in evidence.
    pub(crate) volume_offset: u64,
}

/// Minimal NTFS preview abstraction for bounded range reads.
pub struct NtfsPreviewFile<'a> {
    pub(crate) reader: &'a NtfsReader,
    pub(crate) inode: u64,
}

impl NtfsPreviewFile<'_> {
    pub fn inode(&self) -> u64 {
        self.inode
    }

    pub fn read_range(&self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        self.reader.read_file_data_range(self.inode, offset, length)
    }
}

impl NtfsReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut boot = [0u8; 512];
        reader.read_exact(&mut boot)?;

        if &boot[3..11] != b"NTFS    " {
            return Err(core_invalid_fs_data("not a valid NTFS volume"));
        }

        let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
        let sectors_per_cluster = boot[13];
        if bytes_per_sector == 0 || sectors_per_cluster == 0 {
            return Err(core_invalid_fs_data("invalid NTFS geometry"));
        }
        let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
        let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));
        let _root_dir = root_dir_frn(&boot);
        let mft_record_size = mft_record_bytes(&boot);
        let index_record_size = index_record_bytes(&boot, cluster_size as u32, mft_record_size);

        let mft_record0 = read_contiguous_mft_record(
            &mut *reader,
            offset,
            mft_cluster,
            cluster_size,
            mft_record_size,
            bytes_per_sector,
            0,
        )?;
        let mft_data_runs =
            crate::data_runs::parse_mft_data_runs_from_record(&mft_record0).unwrap_or_default();
        let mft_data_size = crate::utils::parse_mft_data_real_size(&mft_record0).unwrap_or(0);
        let mft_record_count = if mft_record_size == 0 {
            0
        } else {
            mft_data_size / u64::from(mft_record_size)
        };
        let volume_serial = u64::from_le_bytes(boot[0x48..0x50].try_into().unwrap_or([0; 8]));

        Ok(Self {
            reader: RefCell::new(reader),
            bytes_per_sector,
            _sectors_per_cluster: sectors_per_cluster,
            mft_cluster,
            mft_record_size,
            index_record_size,
            cluster_size,
            mft_data_runs,
            mft_record_count,
            volume_serial,
            volume_offset: offset,
        })
    }

    /// Return the underlying evidence reader after filesystem validation.
    ///
    /// Callers that need to validate an MFT identity and then use the reader
    /// for physical range reads can reuse the same opened source.
    pub fn into_reader(self) -> Box<dyn EvidenceReader> {
        self.reader.into_inner()
    }

    /// Convert volume-relative cluster number to absolute evidence offset.
    pub(crate) fn cluster_to_offset(&self, lcn: i64) -> io::Result<u64> {
        if lcn < 0 {
            return Err(core_invalid_fs_data(format!(
                "negative LCN {} in data run",
                lcn
            )));
        }
        Ok(self.volume_offset + lcn as u64 * self.cluster_size)
    }

    /// List children of any directory by MFT inode number.
    /// Reads $INDEX_ROOT (resident) and falls back to $INDEX_ALLOCATION
    /// (non-resident B-Tree) for large directories.
    pub(crate) fn list_dir_by_inode(&self, inode: u64) -> io::Result<Vec<DirEntry>> {
        let rec = self.read_mft_record(inode)?;
        let index_root_entries = self.index_root_entries_from_record(&rec);
        let index_alloc_entries = self.index_allocation_entries(inode, &rec)?;
        if index_alloc_entries.is_empty() && index_root_entries.is_some() {
            let root_count = index_root_entries.as_ref().map_or(0, Vec::len);
            tracing::warn!(
                inode = %inode,
                root_entries = %root_count,
                "NTFS directory has $INDEX_ROOT but no $INDEX_ALLOCATION — large directory listing may be incomplete"
            );
        }

        let mut entries = index_alloc_entries;
        if let Some(root) = index_root_entries {
            entries.extend(root);
        }
        Ok(canonicalize_indx_entries(entries))
    }

    fn index_root_entries_from_record(&self, record: &[u8]) -> Option<Vec<DirEntry>> {
        let mut pos = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
        while pos + 8 < record.len() {
            let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().ok()?);
            if typ == 0xFFFF_FFFF {
                break;
            }
            let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().ok()?) as usize;
            if len == 0 || pos + len > record.len() {
                break;
            }
            if typ == ATTR_TYPE_INDEX_ROOT && pos + 0x18 <= record.len() {
                return self.parse_index_root_entries(record, pos, len).or_else(|| {
                    let entries_off =
                        u32::from_le_bytes(record[pos + 0x10..pos + 0x14].try_into().ok()?)
                            as usize;
                    let entries_start = pos + 0x10 + entries_off;
                    (entries_start < pos + len)
                        .then(|| parse_indx_entries(&record[entries_start..pos + len]))
                });
            }
            pos += len;
        }
        None
    }

    fn parse_index_root_entries(
        &self,
        record: &[u8],
        attr_pos: usize,
        attr_len: usize,
    ) -> Option<Vec<DirEntry>> {
        let content = crate::attribute::resident_attr_content(record, attr_pos, attr_len)?;
        if content.len() < 0x20 {
            return None;
        }

        let entries_off = u32::from_le_bytes(content[0x10..0x14].try_into().ok()?) as usize;
        let entries_end_off = u32::from_le_bytes(content[0x14..0x18].try_into().ok()?) as usize;
        let buffer_end_off = u32::from_le_bytes(content[0x18..0x1C].try_into().ok()?) as usize;
        let entries_start = 0x10usize.saturating_add(entries_off);
        let entries_end = 0x10usize
            .saturating_add(entries_end_off)
            .min(0x10usize.saturating_add(buffer_end_off))
            .min(content.len());
        if entries_start >= entries_end {
            return None;
        }

        Some(parse_indx_entries(&content[entries_start..entries_end]))
    }

    pub fn list_root_children(&self) -> io::Result<Vec<FsNode>> {
        Ok(child_nodes_with_parent_path(
            self.list_dir_by_inode(5)?.into_iter().map(|e| e.node),
            "",
        ))
    }

    pub fn list_root_directory_entries(&self) -> io::Result<Vec<NtfsDirectoryEntry>> {
        self.list_directory_entries_by_inode(5)
    }

    pub fn list_directory_entries_by_inode(
        &self,
        inode: u64,
    ) -> io::Result<Vec<NtfsDirectoryEntry>> {
        Ok(self
            .list_directory_entries_with_sequence_by_inode(inode)?
            .into_iter()
            .map(|(entry, _)| entry)
            .collect())
    }

    pub fn list_directory_entries_with_sequence_by_inode(
        &self,
        inode: u64,
    ) -> io::Result<Vec<(NtfsDirectoryEntry, u16)>> {
        Ok(self
            .list_dir_by_inode(inode)?
            .into_iter()
            .map(|entry| {
                let sequence = entry.mft_sequence;
                (
                    NtfsDirectoryEntry {
                        name: entry.node.name,
                        is_dir: entry.node.is_dir,
                        size: entry.node.size,
                        mft_ref: entry.mft_ref,
                        hidden: entry.node.hidden,
                        system: entry.node.system,
                        read_only: entry.node.read_only,
                        encrypted: entry.node.encrypted,
                        archive: entry.node.archive,
                    },
                    sequence,
                )
            })
            .collect())
    }

    pub fn list_subdir_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        match self.resolve_path(path)? {
            Some(inode) => Ok(child_nodes_with_parent_path(
                self.list_dir_by_inode(inode)?.into_iter().map(|e| e.node),
                path,
            )),
            None => Ok(Vec::new()),
        }
    }
}

impl FileSystemReader for NtfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if path.is_empty() {
            return self.list_root_children();
        }
        self.list_subdir_children(path)
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(self.open_file_cursor(path)?))
    }

    fn open_file_seekable(&self, path: &str) -> io::Result<Box<dyn evidence_core::ReadSeek>> {
        Ok(Box::new(self.open_file_cursor(path)?))
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        NtfsReader::read_file_range(self, path, offset, length)
    }

    fn data_source_name(&self) -> &str {
        "NTFS"
    }
}

impl NtfsReader {
    /// Open a file and return a seekable in-memory cursor. Both `open_file`
    /// and `open_file_seekable` are backed by this helper.
    fn open_file_cursor(&self, path: &str) -> io::Result<io::Cursor<Vec<u8>>> {
        // Fast path: if the path is an MFT inode reference
        // ("mft:NNN" or "mft:PARTITION:NNN"), read directly.
        // This skips INDX name lookups which fail when directories
        // store 8.3 short names instead of long names.
        if let Some(mft_inode) = mft_inode_from_path(path) {
            let data = self.read_file_data(mft_inode)?;
            if data.len() > MAX_BUFFERED_FILE_BYTES {
                return Err(crate::fs_out_of_memory(format!(
                    "file too large to buffer: {} bytes",
                    data.len()
                )));
            }
            return Ok(io::Cursor::new(data));
        }

        let inode = self
            .resolve_file_path(path)?
            .ok_or_else(|| crate::file_not_found(path))?;
        let data = self.read_file_data(inode)?;
        if data.len() > MAX_BUFFERED_FILE_BYTES {
            return Err(crate::fs_out_of_memory(format!(
                "file too large to buffer: {} bytes",
                data.len()
            )));
        }
        Ok(io::Cursor::new(data))
    }
}
