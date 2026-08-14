//! NTFS directory entry types and INDX entry parsing.

use crate::file_name::FileNameNamespace;
use evidence_core::filesystem::fs_node_with_attributes;
use evidence_core::FsNode;
use std::collections::{BTreeMap, BTreeSet};

/// Internal entry with MFT reference for path resolution.
#[derive(Debug)]
pub(crate) struct DirEntry {
    pub(crate) node: FsNode,
    pub(crate) mft_ref: u64,
    pub(crate) mft_sequence: u16,
    pub(crate) namespace: FileNameNamespace,
}

#[derive(Debug, Clone)]
pub struct NtfsDirectoryEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mft_ref: u64,
    pub hidden: bool,
    pub system: bool,
    pub read_only: bool,
    pub encrypted: bool,
    pub archive: bool,
}

/// Parse INDX entries from $INDEX_ROOT buffer. Returns DirEntry with
/// both the FsNode and the child MFT reference (lower 48 bits of file_ref).
pub(crate) fn parse_indx_entries(data: &[u8]) -> Vec<DirEntry> {
    let mut entries = Vec::new();
    let mut off = 0usize;
    while off + 0x52 < data.len() {
        let file_reference = u64::from_le_bytes(data[off..off + 8].try_into().unwrap_or([0; 8]));
        let mft_ref = file_reference & 0x0000_FFFF_FFFF_FFFF;
        let entry_size = u16::from_le_bytes([data[off + 8], data[off + 9]]) as usize;
        if entry_size < 0x52 || off + entry_size > data.len() {
            break;
        }
        let name_len = data[off + 0x50] as usize;
        let namespace = FileNameNamespace::from_raw(data[off + 0x51]);
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
            let mut node = fs_node_with_attributes(
                name, is_dir, size, hidden, system, encrypted, None, None, None,
            );
            node.read_only = flags & 0x01 != 0;
            node.archive = flags & 0x20 != 0;
            entries.push(DirEntry {
                node,
                mft_ref,
                mft_sequence: (file_reference >> 48) as u16,
                namespace,
            });
        }
        off += entry_size;
    }
    entries
}

pub(crate) fn canonicalize_indx_entries(entries: Vec<DirEntry>) -> Vec<DirEntry> {
    let mut entries_by_reference = BTreeMap::<(u64, u16), Vec<DirEntry>>::new();
    for entry in entries {
        entries_by_reference
            .entry((entry.mft_ref, entry.mft_sequence))
            .or_default()
            .push(entry);
    }

    let mut canonical = Vec::new();
    for mut aliases in entries_by_reference.into_values() {
        if aliases
            .iter()
            .any(|entry| entry.namespace.rank() > FileNameNamespace::Dos.rank())
        {
            aliases.retain(|entry| !entry.namespace.is_dos());
        }
        aliases.sort_by(|left, right| {
            right
                .namespace
                .rank()
                .cmp(&left.namespace.rank())
                .then_with(|| left.node.name.cmp(&right.node.name))
        });
        let mut seen_names = BTreeSet::new();
        aliases.retain(|entry| seen_names.insert(entry.node.name.clone()));
        canonical.extend(aliases);
    }
    canonical
}

#[cfg(test)]
#[path = "../tests/unit/directory.rs"]
mod tests;
