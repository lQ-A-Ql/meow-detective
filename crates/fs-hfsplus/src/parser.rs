//! HFS+ on-disk structure parsing helpers.

use crate::constants::*;
use evidence_core::filesystem::truncate_data_to_declared_size;
use std::io::{self, Read, Seek, SeekFrom};

/// Read a big-endian u16 from a byte buffer at the given offset.
pub(crate) fn read_u16_be(buf: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([buf[offset], buf[offset + 1]])
}

/// Read a big-endian u32 from a byte buffer at the given offset.
pub(crate) fn read_u32_be(buf: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

/// Read a big-endian u64 from a byte buffer at the given offset.
pub(crate) fn read_u64_be(buf: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
        buf[offset + 4],
        buf[offset + 5],
        buf[offset + 6],
        buf[offset + 7],
    ])
}

/// Convert an HFS+ timestamp (seconds since 1904-01-01 UTC) to a chrono DateTime.
pub(crate) fn hfs_timestamp_to_dt(
    seconds_since_1904: u32,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let unix_secs = (seconds_since_1904 as i64).checked_sub(MAC_TO_UNIX_EPOCH_OFFSET)?;
    chrono::DateTime::from_timestamp(unix_secs, 0)
}

/// Read a big-endian u16 string length followed by UTF-16BE characters.
pub(crate) fn read_hfs_unicode_str(data: &[u8], offset: usize) -> io::Result<(u16, String)> {
    let char_count = read_u16_be(data, offset) as usize;
    let string_offset = offset + 2;
    let byte_len = char_count * 2;
    if string_offset + byte_len > data.len() {
        return Err(unexpected_fs_eof("HFS+ Unicode string truncated"));
    }
    let mut utf16: Vec<u16> = Vec::with_capacity(char_count);
    for i in 0..char_count {
        utf16.push(read_u16_be(data, string_offset + i * 2));
    }
    let s = String::from_utf16_lossy(&utf16);
    Ok((char_count as u16, s))
}

/// Build an `UnexpectedEof` error with the supplied message.
pub(crate) fn unexpected_fs_eof(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, msg)
}

// ---------------------------------------------------------------------------
// Extent descriptor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct HfsExtentDesc {
    pub(crate) start_block: u32,
    pub(crate) block_count: u32,
}

impl HfsExtentDesc {
    pub(crate) fn from_bytes(data: &[u8], offset: usize) -> Self {
        Self {
            start_block: read_u32_be(data, offset),
            block_count: read_u32_be(data, offset + 4),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HfsForkData {
    pub(crate) logical_size: u64,
    pub(crate) extents: Vec<HfsExtentDesc>,
}

impl HfsForkData {
    pub(crate) fn from_bytes(data: &[u8], fork_offset: usize) -> Self {
        let logical_size = read_u64_be(data, fork_offset + FORK_LOGICAL_SIZE);
        let _total_blocks = read_u32_be(data, fork_offset + FORK_TOTAL_BLOCKS);
        let mut extents = Vec::new();
        for i in 0..8 {
            let off = fork_offset + FORK_EXTENTS + i * EXTENT_DESC_SIZE;
            let ed = HfsExtentDesc::from_bytes(data, off);
            if ed.block_count > 0 {
                extents.push(ed);
            }
        }
        Self {
            logical_size,
            extents,
        }
    }

    /// Read all data for this fork from the underlying reader.
    pub(crate) fn read_all<R: Read + Seek>(
        &self,
        reader: &mut R,
        volume_offset: u64,
        block_size: u32,
    ) -> io::Result<Vec<u8>> {
        if self.logical_size == 0 {
            return Ok(Vec::new());
        }
        let mut data = Vec::new();
        for extent in &self.extents {
            let offset = volume_offset + extent.start_block as u64 * block_size as u64;
            let size = extent.block_count as u64 * block_size as u64;
            reader.seek(SeekFrom::Start(offset))?;
            let mut chunk = vec![0u8; size as usize];
            reader.read_exact(&mut chunk)?;
            data.extend_from_slice(&chunk);
        }
        Ok(truncate_data_to_declared_size(data, self.logical_size))
    }
}

// ---------------------------------------------------------------------------
// Catalog record helpers
// ---------------------------------------------------------------------------

/// A parsed catalog B-tree record (folder or file).
#[derive(Debug)]
pub(crate) struct CatalogRecord {
    pub(crate) name: String,
    pub(crate) cnid: u32,
    pub(crate) is_dir: bool,
    /// For files: the data fork for reading content.
    pub(crate) data_fork: Option<HfsForkData>,
    /// Logical file size.
    pub(crate) logical_size: u64,
    /// HFS+ timestamps.
    pub(crate) create_date: u32,
    pub(crate) content_mod_date: u32,
    pub(crate) access_date: u32,
    /// BSD file mode (parsed for format completeness).
    pub(crate) _file_mode: u16,
    /// BSD special device ID (parsed for format completeness).
    pub(crate) _special: u32,
}

/// Parse a single B-tree record (key + data) into a CatalogRecord.
pub(crate) fn parse_catalog_record(
    key_data: &[u8],
    data: &[u8],
) -> io::Result<Option<CatalogRecord>> {
    if data.len() < 2 {
        return Ok(None);
    }
    let record_type = i16::from_be_bytes([data[0], data[1]]);

    // Parse the key: keyLength(u16) + parentCNID(u32) + nodeName(HFSUniStr255).
    // We read from key_data[0] which starts at the keyLength field.
    if key_data.len() < 8 {
        return Ok(None);
    }
    let _key_length = read_u16_be(key_data, 0);
    let _parent_cnid = read_u32_be(key_data, 2);

    // nodeName starts at offset 6: u16 char count + UTF-16BE chars.
    let (char_count, name) = read_hfs_unicode_str(key_data, 6)?;
    if name.is_empty() && char_count == 0 {
        // Thread record — skip for listing; we only care about folder/file records.
        return Ok(None);
    }

    match record_type {
        RECORD_TYPE_FOLDER => {
            if data.len() < FOLDER_RECORD_SIZE {
                return Ok(None);
            }
            let cnid = read_u32_be(data, FOLDER_ID);
            let create_date = read_u32_be(data, FOLDER_CREATE_DATE);
            let content_mod_date = read_u32_be(data, FOLDER_CONTENT_MOD_DATE);
            let access_date = read_u32_be(data, FOLDER_ACCESS_DATE);
            let _file_mode = read_u16_be(data, FOLDER_PERMISSIONS + BSDINFO_FILE_MODE);
            let _special = read_u32_be(data, FOLDER_PERMISSIONS + BSDINFO_SPECIAL);
            Ok(Some(CatalogRecord {
                name,
                cnid,
                is_dir: true,
                data_fork: None,
                logical_size: 0,
                create_date,
                content_mod_date,
                access_date,
                _file_mode,
                _special,
            }))
        }
        RECORD_TYPE_FILE => {
            // Ensure we have room for the data fork structure.
            if data.len() < FILE_DATA_FORK + 80 {
                return Ok(None);
            }
            let cnid = read_u32_be(data, FILE_ID);
            let create_date = read_u32_be(data, FILE_CREATE_DATE);
            let content_mod_date = read_u32_be(data, FILE_CONTENT_MOD_DATE);
            let access_date = read_u32_be(data, FILE_ACCESS_DATE);
            let _file_mode = read_u16_be(data, FILE_PERMISSIONS + BSDINFO_FILE_MODE);
            let _special = read_u32_be(data, FILE_PERMISSIONS + BSDINFO_SPECIAL);
            let data_fork = HfsForkData::from_bytes(data, FILE_DATA_FORK);
            let logical_size = data_fork.logical_size;
            Ok(Some(CatalogRecord {
                name,
                cnid,
                is_dir: false,
                data_fork: Some(data_fork),
                logical_size,
                create_date,
                content_mod_date,
                access_date,
                _file_mode,
                _special,
            }))
        }
        _ => {
            // Thread records and unknown — skip.
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// B-tree node parsing
// ---------------------------------------------------------------------------

/// Minimal B-tree node descriptor.
#[derive(Debug)]
pub(crate) struct BtNodeDesc {
    /// Forward link to next B-tree node (parsed for format completeness).
    pub(crate) _f_link: u32,
    /// Backward link to previous B-tree node (parsed for format completeness).
    pub(crate) _b_link: u32,
    pub(crate) kind: u8,
    /// Height of this node in the B-tree (parsed for format completeness).
    pub(crate) _height: u8,
    pub(crate) num_records: u16,
    /// Offset from start of node data to the first record offset slot.
    pub(crate) record_offsets_start: usize,
}

impl BtNodeDesc {
    pub(crate) fn parse(node_data: &[u8]) -> io::Result<Self> {
        if node_data.len() < BT_NODE_DESC_SIZE {
            return Err(unexpected_fs_eof("BT node descriptor too short"));
        }
        Ok(Self {
            _f_link: read_u32_be(node_data, BT_F_LINK),
            _b_link: read_u32_be(node_data, BT_B_LINK),
            kind: node_data[BT_KIND],
            _height: node_data[BT_HEIGHT],
            num_records: read_u16_be(node_data, BT_NUM_RECORDS),
            // reserved field at BT_RESERVED is ignored.
            record_offsets_start: BT_NODE_DESC_SIZE,
        })
    }

    /// Return the record offsets as a vector of u16.
    pub(crate) fn record_offsets(&self, node_data: &[u8]) -> Vec<u16> {
        let mut offsets = Vec::with_capacity(self.num_records as usize);
        for i in 0..self.num_records as usize {
            let off = self.record_offsets_start + i * 2;
            if off + 2 <= node_data.len() {
                offsets.push(read_u16_be(node_data, off));
            }
        }
        offsets
    }

    /// Extract the record data (key + value) at a given internal offset.
    /// For index nodes, the "value" is just a u32 child node number.
    /// For leaf nodes, the key has parentCNID+nodeName and the value is a
    /// catalog folder/file record.
    pub(crate) fn get_record_data<'a>(&self, node_data: &'a [u8], record_offset: u16) -> &'a [u8] {
        let start = record_offset as usize;
        if start >= node_data.len() {
            return &[];
        }
        &node_data[start..]
    }
}
