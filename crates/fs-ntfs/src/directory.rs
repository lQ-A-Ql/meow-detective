//! NTFS directory entry types and INDX entry parsing.

use evidence_core::filesystem::fs_node_with_attributes;
use evidence_core::FsNode;

/// Internal entry with MFT reference for path resolution.
pub(crate) struct DirEntry {
    pub(crate) node: FsNode,
    pub(crate) mft_ref: u64,
}

#[derive(Debug, Clone)]
pub struct NtfsDirectoryEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mft_ref: u64,
    pub hidden: bool,
    pub system: bool,
    pub encrypted: bool,
}

/// Parse INDX entries from $INDEX_ROOT buffer. Returns DirEntry with
/// both the FsNode and the child MFT reference (lower 48 bits of file_ref).
pub(crate) fn parse_indx_entries(data: &[u8]) -> Vec<DirEntry> {
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
            let flags = if off + 0x4C <= data.len() && off + 0x4C <= off + entry_size {
                u32::from_le_bytes(data[off + 0x48..off + 0x4C].try_into().unwrap_or([0; 4]))
            } else {
                0
            };
            let is_dir = flags & 0x10000000 != 0;
            let hidden = flags & 0x02 != 0;
            let system = flags & 0x04 != 0;
            let encrypted = flags & 0x4000 != 0;
            let size = if is_dir || off + 0x48 > data.len() {
                0
            } else {
                u64::from_le_bytes(data[off + 0x40..off + 0x48].try_into().unwrap_or([0; 8]))
            };
            entries.push(DirEntry {
                node: fs_node_with_attributes(
                    name, is_dir, size, hidden, system, encrypted, None, None, None,
                ),
                mft_ref,
            });
        }
        off += entry_size;
    }
    entries
}
