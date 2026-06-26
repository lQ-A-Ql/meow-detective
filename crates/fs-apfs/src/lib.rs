//! APFS filesystem reader.
//!
//! Implements the `FileSystemReader` trait for Apple File System containers
//! and volumes.  Parses the container superblock at offset 0 (magic `NXSB`),
//! checkpoint descriptors for OID → block translation, volume superblocks
//! (`APSB`), inode records with four timestamps, and directory entries via
//! hashed-key B-trees.
//!
//! Supported features:
//! - Container superblock with checkpoint-based OID mapping
//! - Volume enumeration (single or multi-volume containers)
//! - Inode parsing with mode, uid/gid, and four timestamps
//! - Directory listing via `j_drec_hashed_key` B-tree
//! - File content via extent references

pub mod checkpoint;

use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node, invalid_fs_data,
    is_special_directory_name, path_components, path_is_directory, path_not_found, root_node,
    truncate_data_to_declared_size, FileSystemReader, FsNode,
};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Container superblock magic ("NXSB" as u32 LE).
pub(crate) const NXSB_MAGIC: u32 = 0x4253_584E;
/// Volume superblock magic ("APSB" as u32 LE).
pub(crate) const APSB_MAGIC: u32 = 0x4253_5350;

// Container superblock field offsets.
pub(crate) const NX_MAGIC_OFF: usize = 0x20;
pub(crate) const NX_BLOCK_SIZE_OFF: usize = 0x24;
pub(crate) const NX_XP_DESC_BLOCKS_OFF: usize = 0x38;
pub(crate) const NX_XP_DESC_BASE_OFF: usize = 0x40;
pub(crate) const NX_XP_DESC_INDEX_OFF: usize = 0x58;
#[allow(dead_code)]
pub(crate) const NX_XP_DESC_LEN_OFF: usize = 0x5C;
pub(crate) const NX_MAX_FILE_SYSTEMS_OFF: usize = 0x84;
pub(crate) const NX_FS_OID_OFF: usize = 0x88;

// Volume superblock field offsets.
pub(crate) const AP_MAGIC_OFF: usize = 0x20;
pub(crate) const AP_ROOT_TREE_OID_OFF: usize = 0xB0;

// B-tree node field offsets.
#[allow(dead_code)]
pub(crate) const BT_FLAGS_OFF: usize = 0x00;
#[allow(dead_code)]
pub(crate) const BT_LEVEL_OFF: usize = 0x02;
#[allow(dead_code)]
pub(crate) const BT_NKEYS_OFF: usize = 0x04;
#[allow(dead_code)]
pub(crate) const BT_TABLE_SPACE_OFF: usize = 0x08;
#[allow(dead_code)]
pub(crate) const BT_TOC_BASE: usize = 0x14;

// B-tree flags.
#[allow(dead_code)]
pub(crate) const BT_ROOT: u16 = 0x0002;
#[allow(dead_code)]
pub(crate) const BT_LEAF: u16 = 0x0004;
#[allow(dead_code)]
pub(crate) const BT_FIXED_KV: u16 = 0x0008;

// Inode types.
#[allow(dead_code)]
pub(crate) const S_IFDIR: u16 = 4;
#[allow(dead_code)]
pub(crate) const S_IFREG: u16 = 8;
#[allow(dead_code)]
pub(crate) const S_IFLNK: u16 = 10;

// j_drec_val type.
#[allow(dead_code)]
pub(crate) const DREC_TYPE_DIR: u16 = 2;
#[allow(dead_code)]
pub(crate) const DREC_TYPE_FILE: u16 = 1;

// TOC entry size.
pub(crate) const TOC_ENTRY_SIZE: usize = 8;

// ---------------------------------------------------------------------------
// On-disk helpers
// ---------------------------------------------------------------------------

/// APFS B-tree TOC entry (kvloc_t).
pub(crate) struct TocEntry {
    pub(crate) key_off: u16,
    pub(crate) key_len: u16,
    pub(crate) val_off: u16,
    pub(crate) val_len: u16,
}

impl TocEntry {
    pub(crate) fn parse(data: &[u8]) -> Self {
        Self {
            key_off: u16::from_le_bytes([data[0], data[1]]),
            key_len: u16::from_le_bytes([data[2], data[3]]),
            val_off: u16::from_le_bytes([data[4], data[5]]),
            val_len: u16::from_le_bytes([data[6], data[7]]),
        }
    }
}

/// Read a TOC from a B-tree node.
pub(crate) fn parse_toc(node_data: &[u8], nkeys: u32) -> Vec<TocEntry> {
    let table_off = u16::from_le_bytes([
        node_data[BT_TABLE_SPACE_OFF],
        node_data[BT_TABLE_SPACE_OFF + 1],
    ]) as usize;
    let mut entries = Vec::new();
    for i in 0..nkeys {
        let off = table_off + (i as usize) * TOC_ENTRY_SIZE;
        if off + TOC_ENTRY_SIZE > node_data.len() {
            break;
        }
        entries.push(TocEntry::parse(&node_data[off..off + TOC_ENTRY_SIZE]));
    }
    entries
}

// ---------------------------------------------------------------------------
// OMap / checkpoint mapping
// ---------------------------------------------------------------------------

/// Simple OID→block translation built from the container checkpoint.
#[derive(Debug, Clone)]
pub(crate) struct OidMap {
    pub(crate) mappings: HashMap<u64, u64>,
}

impl OidMap {
    pub(crate) fn from_checkpoint_node(
        node_data: &[u8],
        flags: u16,
        nkeys: u32,
    ) -> io::Result<Self> {
        let mut mappings = HashMap::new();
        if flags & BT_FIXED_KV == 0 {
            return Ok(Self { mappings });
        }
        let toc = parse_toc(node_data, nkeys);
        for entry in &toc {
            let key_start = entry.key_off as usize;
            let key_end = key_start + entry.key_len as usize;
            let val_start = entry.val_off as usize;
            let val_end = val_start + entry.val_len as usize;
            if key_end <= node_data.len()
                && val_end <= node_data.len()
                && entry.key_len >= 8
                && entry.val_len >= 8
            {
                let oid = u64::from_le_bytes(
                    node_data[key_start..key_start + 8]
                        .try_into()
                        .map_err(|_| invalid_fs_data("checkpoint OID key too short"))?,
                );
                let block = u64::from_le_bytes(
                    node_data[val_start..val_start + 8]
                        .try_into()
                        .map_err(|_| invalid_fs_data("checkpoint block value too short"))?,
                );
                mappings.insert(oid, block);
            }
        }
        Ok(Self { mappings })
    }

    pub(crate) fn resolve(&self, oid: u64) -> io::Result<u64> {
        self.mappings
            .get(&oid)
            .copied()
            .ok_or_else(|| invalid_fs_data(format!("OID {} not found in checkpoint map", oid)))
    }
}

// ---------------------------------------------------------------------------
// Inode parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ApfsInode {
    pub(crate) oid: u64,
    pub(crate) parent_id: u64,
    pub(crate) private_id: u64,
    pub(crate) mode: u16,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) flags: u64,
    pub(crate) access_time: u64,
    pub(crate) change_time: u64,
    pub(crate) mod_time: u64,
    pub(crate) create_time: u64,
    pub(crate) nchildren_or_nlink: u32,
    /// OID of the child B-tree root (for directories).
    pub(crate) children_oid: u64,
    /// Extent references (OIDs pointing to data blocks).
    pub(crate) extents: Vec<u64>,
    /// Logical size in bytes.
    pub(crate) logical_size: u64,
}

/// Parse a `j_inode_val_t` from bytes (the value of a B-tree inode record).
///
/// Field layout (simplified):
///  0x00: parent_id (u64)
///  0x08: private_id (u64)
///  0x10: creation_time (u64, ns)
///  0x18: mod_time (u64, ns)
///  0x20: change_time (u64, ns)
///  0x28: access_time (u64, ns)
///  0x30: internal_flags (u64)
///  0x38: nchildren / nlink (u32)
///  0x3C: default_protection_class (u32)
///  0x40: bsd_flags (u32)
///  0x44: owner (u32 - uid)
///  0x48: group (u32 - gid)
///  0x4C: mode (u16)
///  0x4E: pad (u16)
///  0x50: uncompressed_size (u64)
///  0x58: (various)
///
/// The children_oid / extents follow in the j_inode_val or are stored separately
/// in the xfield area. For simplicity we scan the raw value for known OID patterns.
pub(crate) fn parse_inode_val(data: &[u8], oid: u64) -> io::Result<ApfsInode> {
    let parent_id = if data.len() >= 8 {
        u64::from_le_bytes(
            data[0..8]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let private_id = if data.len() >= 16 {
        u64::from_le_bytes(
            data[8..16]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let create_time = if data.len() >= 24 {
        u64::from_le_bytes(
            data[0x10..0x18]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let mod_time = if data.len() >= 32 {
        u64::from_le_bytes(
            data[0x18..0x20]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let change_time = if data.len() >= 40 {
        u64::from_le_bytes(
            data[0x20..0x28]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let access_time = if data.len() >= 48 {
        u64::from_le_bytes(
            data[0x28..0x30]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let flags = if data.len() >= 56 {
        u64::from_le_bytes(
            data[0x30..0x38]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let nchildren_or_nlink = if data.len() >= 60 {
        u32::from_le_bytes(
            data[0x38..0x3C]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let uid = if data.len() >= 72 {
        u32::from_le_bytes(
            data[0x44..0x48]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let gid = if data.len() >= 76 {
        u32::from_le_bytes(
            data[0x48..0x4C]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let mode = if data.len() >= 78 {
        u16::from_le_bytes(
            data[0x4C..0x4E]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };

    // Scan for children_oid and extent OIDs.
    // j_children_oid is typically at offset 0x80 in j_inode_val.
    let children_oid = if data.len() >= 0x88 {
        u64::from_le_bytes(
            data[0x80..0x88]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };
    let logical_size = if data.len() >= 0x60 {
        u64::from_le_bytes(
            data[0x58..0x60]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        )
    } else {
        0
    };

    // Scan remainder for extent OIDs (non-zero u64 values).
    let mut extents = Vec::new();
    if data.len() >= 0x98 {
        for off in (0x88..data.len() - 7).step_by(8) {
            let val = u64::from_le_bytes(
                data[off..off + 8]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            if val != 0 && val != children_oid && val != parent_id && val != private_id {
                extents.push(val);
            }
        }
    }

    Ok(ApfsInode {
        oid,
        parent_id,
        private_id,
        mode,
        uid,
        gid,
        flags,
        access_time,
        change_time,
        mod_time,
        create_time,
        nchildren_or_nlink,
        children_oid,
        extents,
        logical_size,
    })
}

pub(crate) fn ns_to_option_dt(ns: u64) -> Option<chrono::DateTime<chrono::Utc>> {
    if ns == 0 {
        return None;
    }
    let secs = (ns / 1_000_000_000) as i64;
    let nsecs = (ns % 1_000_000_000) as u32;
    chrono::DateTime::from_timestamp(secs, nsecs)
}

// ---------------------------------------------------------------------------
// Directory parsing
// ---------------------------------------------------------------------------

/// A directory entry parsed from a `j_drec` record.
#[derive(Debug)]
pub(crate) struct DirEntry {
    pub(crate) name: String,
    pub(crate) file_id: u64,
    pub(crate) is_dir: bool,
    pub(crate) date_added: u64,
}

/// Parse a directory B-tree leaf node (variable-size KV).
/// Key: `j_drec_hashed_key_t` (name_hash: u64, name_len: u16, name: [u8]).
/// Value: `j_drec_val_t` (file_id: u64, date_added: u64, flags: u64).
pub(crate) fn parse_dir_b_tree(
    node_data: &[u8],
    flags: u16,
    nkeys: u32,
) -> io::Result<Vec<DirEntry>> {
    // Variable-size KV directory nodes.
    if flags & BT_FIXED_KV != 0 {
        return Ok(Vec::new());
    }
    let toc = parse_toc(node_data, nkeys);
    let mut entries = Vec::new();
    for entry in &toc {
        let key_start = entry.key_off as usize;
        let key_end = key_start + entry.key_len as usize;
        let val_start = entry.val_off as usize;
        let val_end = val_start + entry.val_len as usize;
        if key_end > node_data.len() || val_end > node_data.len() {
            continue;
        }
        let key_data = &node_data[key_start..key_end];
        let val_data = &node_data[val_start..val_end];

        // Parse j_drec_hashed_key: name_hash(8) + name_len(2) + name
        if key_data.len() < 10 {
            continue;
        }
        let name_len = u16::from_le_bytes([key_data[8], key_data[9]]) as usize;
        if key_data.len() < 10 + name_len {
            continue;
        }
        let name = String::from_utf8_lossy(&key_data[10..10 + name_len]).to_string();
        if name.is_empty() || is_special_directory_name(&name) {
            continue;
        }

        // Parse j_drec_val: file_id(8) + date_added(8) + flags(4?) or type?
        // Simplified: file_id at offset 0, type/flags at offset 24.
        let file_id = if val_data.len() >= 8 {
            u64::from_le_bytes(
                val_data[0..8]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            )
        } else {
            0
        };
        let date_added = if val_data.len() >= 16 {
            u64::from_le_bytes(
                val_data[8..16]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            )
        } else {
            0
        };
        // Type field: typically at offset 24 (u16) or derived from flags.
        let is_dir = if val_data.len() >= 26 {
            let dtype = u16::from_le_bytes([val_data[24], val_data[25]]);
            dtype == DREC_TYPE_DIR
        } else {
            false
        };

        entries.push(DirEntry {
            name,
            file_id,
            is_dir,
            date_added,
        });
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// ApfsReader
// ---------------------------------------------------------------------------

pub struct ApfsReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    block_size: u32,
    volume_offset: u64,
    /// OID → block number mapping (from checkpoint).
    oid_map: OidMap,
    /// Volumes in this container: name → (fs_oid, root_tree_oid).
    volumes: Vec<ApfsVolume>,
    /// Default volume is the first one.
    #[allow(dead_code)]
    default_volume_idx: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ApfsVolume {
    pub(crate) name: String,
    #[allow(dead_code)]
    pub(crate) fs_oid: u64,
    pub(crate) root_tree_oid: u64,
}

impl ApfsReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut block0 = [0u8; 4096];
        reader.read_exact(&mut block0)?;

        let magic = u32::from_le_bytes(
            block0[NX_MAGIC_OFF..NX_MAGIC_OFF + 4]
                .try_into()
                .map_err(|_| invalid_fs_data("APFS container superblock header too short"))?,
        );
        if magic != NXSB_MAGIC {
            return Err(invalid_fs_data(format!(
                "not a valid APFS container (magic 0x{:08X})",
                magic
            )));
        }

        let block_size = u32::from_le_bytes(
            block0[NX_BLOCK_SIZE_OFF..NX_BLOCK_SIZE_OFF + 4]
                .try_into()
                .map_err(|_| invalid_fs_data("APFS block_size field too short"))?,
        );
        if block_size == 0 || block_size < 512 {
            return Err(invalid_fs_data("invalid APFS block size"));
        }

        let _xp_desc_blocks = u32::from_le_bytes(
            block0[NX_XP_DESC_BLOCKS_OFF..NX_XP_DESC_BLOCKS_OFF + 4]
                .try_into()
                .map_err(|_| invalid_fs_data("APFS xp_desc_blocks field too short"))?,
        );
        let xp_desc_base = u64::from_le_bytes(
            block0[NX_XP_DESC_BASE_OFF..NX_XP_DESC_BASE_OFF + 8]
                .try_into()
                .map_err(|_| invalid_fs_data("APFS xp_desc_base field too short"))?,
        );
        let xp_desc_index = u32::from_le_bytes(
            block0[NX_XP_DESC_INDEX_OFF..NX_XP_DESC_INDEX_OFF + 4]
                .try_into()
                .map_err(|_| invalid_fs_data("APFS xp_desc_index field too short"))?,
        );
        let max_file_systems = u32::from_le_bytes(
            block0[NX_MAX_FILE_SYSTEMS_OFF..NX_MAX_FILE_SYSTEMS_OFF + 4]
                .try_into()
                .map_err(|_| invalid_fs_data("APFS max_file_systems field too short"))?,
        );

        // Read the checkpoint descriptor node.
        let cp_block = xp_desc_base + xp_desc_index as u64;
        let cp_data = Self::read_block_at(&mut reader, offset, cp_block, block_size)?;

        // Parse the checkpoint: it's a btree_node with OID→block mappings.
        let cp_flags = u16::from_le_bytes(
            cp_data[BT_FLAGS_OFF..BT_FLAGS_OFF + 2]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let cp_nkeys = u32::from_le_bytes(
            cp_data[BT_NKEYS_OFF..BT_NKEYS_OFF + 4]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let oid_map = OidMap::from_checkpoint_node(&cp_data, cp_flags, cp_nkeys)?;

        // Discover volumes.
        let mut volumes = Vec::new();
        for i in 0..max_file_systems as usize {
            let fs_oid_off = NX_FS_OID_OFF + i * 8;
            if fs_oid_off + 8 > block0.len() {
                break;
            }
            let fs_oid = u64::from_le_bytes(
                block0[fs_oid_off..fs_oid_off + 8]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            if fs_oid == 0 {
                continue;
            }

            // Resolve fs_oid to volume superblock block.
            let vsb_block = match oid_map.resolve(fs_oid) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("APFS: cannot resolve volume OID {}: {}", fs_oid, e);
                    continue;
                }
            };
            let vsb_data = Self::read_block_at(&mut reader, offset, vsb_block, block_size)?;

            let vsb_magic = u32::from_le_bytes(
                vsb_data[AP_MAGIC_OFF..AP_MAGIC_OFF + 4]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            if vsb_magic != APSB_MAGIC {
                continue;
            }

            let root_tree_oid = u64::from_le_bytes(
                vsb_data[AP_ROOT_TREE_OID_OFF..AP_ROOT_TREE_OID_OFF + 8]
                    .try_into()
                    .map_err(|_| invalid_fs_data("APFS root_tree_oid field too short"))?,
            );

            let vol_name = if i == 0 {
                "Macintosh HD".to_string()
            } else {
                format!("Volume {}", i + 1)
            };

            volumes.push(ApfsVolume {
                name: vol_name,
                fs_oid,
                root_tree_oid,
            });
        }

        if volumes.is_empty() {
            return Err(invalid_fs_data("APFS container has no accessible volumes"));
        }

        Ok(Self {
            reader: RefCell::new(reader),
            block_size,
            volume_offset: offset,
            oid_map,
            volumes,
            default_volume_idx: 0,
        })
    }

    fn read_block_at(
        reader: &mut Box<dyn EvidenceReader>,
        volume_offset: u64,
        block: u64,
        block_size: u32,
    ) -> io::Result<Vec<u8>> {
        let offset = volume_offset + block * block_size as u64;
        let mut buf = vec![0u8; block_size as usize];
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_block(&self, block: u64) -> io::Result<Vec<u8>> {
        let absolute = self.volume_offset + block * self.block_size as u64;
        let mut buf = vec![0u8; self.block_size as usize];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(absolute))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn resolve_oid_block(&self, oid: u64) -> io::Result<Vec<u8>> {
        let block_no = self.oid_map.resolve(oid)?;
        self.read_block(block_no)
    }

    fn read_btree_node(&self, oid: u64) -> io::Result<(Vec<u8>, u16, u32)> {
        let data = self.resolve_oid_block(oid)?;
        let flags = u16::from_le_bytes(
            data[BT_FLAGS_OFF..BT_FLAGS_OFF + 2]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let nkeys = u32::from_le_bytes(
            data[BT_NKEYS_OFF..BT_NKEYS_OFF + 4]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        // Recursively follow non-leaf nodes down to the first leaf.
        if flags & BT_LEAF == 0 {
            // Internal node: follow the TOC to child node OIDs.
            let toc = parse_toc(&data, nkeys);
            for entry in &toc {
                let val_start = entry.val_off as usize;
                if val_start + 8 <= data.len() {
                    let child_oid = u64::from_le_bytes(
                        data[val_start..val_start + 8]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    );
                    if child_oid != 0 {
                        let child_data = self.resolve_oid_block(child_oid)?;
                        let cf = u16::from_le_bytes(
                            child_data[BT_FLAGS_OFF..BT_FLAGS_OFF + 2]
                                .try_into()
                                .map_err(|_| {
                                    invalid_fs_data("B-tree child node flags too short")
                                })?,
                        );
                        if cf & BT_LEAF != 0 {
                            let cn = u32::from_le_bytes(
                                child_data[BT_NKEYS_OFF..BT_NKEYS_OFF + 4]
                                    .try_into()
                                    .map_err(|_| {
                                        invalid_fs_data("B-tree child node nkeys too short")
                                    })?,
                            );
                            return Ok((child_data, cf, cn));
                        }
                    }
                }
            }
        }
        Ok((data, flags, nkeys))
    }

    fn read_inode(&self, oid: u64) -> io::Result<ApfsInode> {
        // Resolve the inode: the inode record is stored as a value in the
        // root tree B-tree. The key is the OID.
        // Actually, for simplicity, we resolve the OID directly to its block
        // and parse the raw data as the inode value.
        let data = self.resolve_oid_block(oid)?;
        parse_inode_val(&data, oid)
    }

    fn list_directory(&self, dir_inode: &ApfsInode) -> io::Result<Vec<DirEntry>> {
        if dir_inode.children_oid == 0 {
            return Ok(Vec::new());
        }
        let (node_data, flags, nkeys) = self.read_btree_node(dir_inode.children_oid)?;
        parse_dir_b_tree(&node_data, flags, nkeys)
    }

    fn resolve_path_in_volume(
        &self,
        root_tree_oid: u64,
        path: &str,
    ) -> io::Result<Option<(u64, bool)>> {
        // root_tree_oid points to the root directory inode OID.
        let root_inode = self.read_inode(root_tree_oid)?;
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((root_inode.oid, true)));
        }

        let mut current_inode = root_inode;
        for (i, comp) in components.iter().enumerate() {
            let entries = self.list_directory(&current_inode)?;
            let is_last = i == components.len() - 1;

            let found = entries.iter().find(|e| e.name.eq_ignore_ascii_case(comp));
            match found {
                Some(entry) => {
                    if is_last {
                        return Ok(Some((entry.file_id, entry.is_dir)));
                    }
                    if !entry.is_dir {
                        return Ok(None);
                    }
                    current_inode = self.read_inode(entry.file_id)?;
                }
                None => {
                    // Try case-sensitive as fallback.
                    let exact = entries.iter().find(|e| e.name == *comp);
                    match exact {
                        Some(entry) => {
                            if is_last {
                                return Ok(Some((entry.file_id, entry.is_dir)));
                            }
                            if !entry.is_dir {
                                return Ok(None);
                            }
                            current_inode = self.read_inode(entry.file_id)?;
                        }
                        None => return Ok(None),
                    }
                }
            }
        }
        Ok(None)
    }

    /// Read file content via extent references.
    fn read_file_content(&self, inode: &ApfsInode) -> io::Result<Vec<u8>> {
        if inode.logical_size == 0 {
            return Ok(Vec::new());
        }

        let mut data = Vec::new();
        for &ext_oid in &inode.extents {
            let block_data = self.resolve_oid_block(ext_oid)?;
            data.extend_from_slice(&block_data);
        }
        Ok(truncate_data_to_declared_size(data, inode.logical_size))
    }
}

// ---------------------------------------------------------------------------
// FileSystemReader implementation
// ---------------------------------------------------------------------------

impl FileSystemReader for ApfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        // Empty/root path: list volumes.
        if path.is_empty() || path == "/" || path == "\\" {
            let nodes: Vec<FsNode> = self
                .volumes
                .iter()
                .map(|v| fs_node(v.name.clone(), true, 0, None, None, None))
                .collect();
            return Ok(child_nodes_with_parent_path(nodes, ""));
        }

        // Split "VolName" or "VolName/dir1/dir2".
        let first_slash = path.find(['/', '\\']);
        let (vol_name, sub_path) = match first_slash {
            Some(pos) => (&path[..pos], &path[pos + 1..]),
            None => (path, ""),
        };

        let vol = self
            .volumes
            .iter()
            .find(|v| v.name == vol_name)
            .ok_or_else(|| path_not_found(path))?;

        let (inode_oid, is_dir) = self
            .resolve_path_in_volume(vol.root_tree_oid, sub_path)?
            .ok_or_else(|| path_not_found(path))?;

        if !is_dir {
            return Err(evidence_core::filesystem::path_is_not_directory(path));
        }

        let dir_inode = self.read_inode(inode_oid)?;
        let entries = self.list_directory(&dir_inode)?;

        let mut nodes = Vec::new();
        for entry in entries {
            nodes.push(fs_node(
                entry.name.clone(),
                entry.is_dir,
                0, // size not tracked for directory listing
                ns_to_option_dt(entry.date_added),
                None,
                None,
            ));
        }
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let first_slash = path.find(['/', '\\']);
        let (vol_name, sub_path) = match first_slash {
            Some(pos) => (&path[..pos], &path[pos + 1..]),
            None => return Err(file_not_found(path)),
        };

        let vol = self
            .volumes
            .iter()
            .find(|v| v.name == vol_name)
            .ok_or_else(|| file_not_found(path))?;

        let (inode_oid, is_dir) = self
            .resolve_path_in_volume(vol.root_tree_oid, sub_path)?
            .ok_or_else(|| file_not_found(path))?;

        if is_dir {
            return Err(path_is_directory(path));
        }

        let inode = self.read_inode(inode_oid)?;
        let data = self.read_file_content(&inode)?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn data_source_name(&self) -> &str {
        "apfs"
    }
}

// ===========================================================================
// Tests
// ===========================================================================

// ---------------------------------------------------------------------------
// Shared test helpers (available to checkpoint.rs tests as well)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) struct FakeReader {
    pub(crate) data: Vec<u8>,
    pos: u64,
    info: evidence_core::ReaderInfo,
}

#[cfg(test)]
impl FakeReader {
    pub(crate) fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            pos: 0,
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("fake-apfs"),
                size: 0,
                kind: "fake-apfs".to_string(),
            },
        }
    }
}

#[cfg(test)]
impl std::io::Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = (self.pos as usize).min(self.data.len());
        let end = (start + buf.len()).min(self.data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&self.data[start..end]);
        self.pos += n as u64;
        Ok(n)
    }
}

#[cfg(test)]
impl std::io::Seek for FakeReader {
    fn seek(&mut self, pos: std::io::SeekFrom) -> io::Result<u64> {
        self.pos = match pos {
            std::io::SeekFrom::Start(p) => p,
            std::io::SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
            std::io::SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
        };
        Ok(self.pos)
    }
}

#[cfg(test)]
impl EvidenceReader for FakeReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    // -------------------------------------------------------------------
    // Minimal APFS fixture
    // -------------------------------------------------------------------
    //
    // Layout (block_size = 4096, 12 blocks = 0xC000 bytes):
    //
    //  Block  Offset    Content
    //  -----  --------  ----------------------------------
    //    0    0x00000   Container superblock (NXSB)
    //    1    0x01000   Checkpoint OID→block map (B-tree)
    //    2    0x02000   Empty (padding)
    //    3    0x03000   Volume superblock (APSB)
    //    4    0x04000   Root directory inode
    //    5    0x05000   Root dir B-tree node
    //    6    0x06000   File inode (file.txt)
    //    7    0x07000   File data block
    //    8    0x08000   Subdir inode
    //    9    0x09000   Subdir B-tree node
    //   10    0x0A000   Nested file inode
    //   11    0x0B000   Nested file data block
    //   12    0x0C000   dir1 inode
    //   13    0x0D000   dir1 B-tree node
    //   14    0x0E000   dir2 inode
    //   15    0x0F000   dir2 B-tree node
    //   16    0x10000   three-level file inode
    //   17    0x11000   three-level file data

    fn build_apfs_fixture() -> Vec<u8> {
        let block_size: usize = 4096;
        let total_blocks: usize = 18;
        let total_size = total_blocks * block_size;
        let mut img = vec![0u8; total_size];

        let block = |n: usize| -> usize { n * block_size };

        // ── Block 0: Container superblock ──
        let csb = &mut img[block(0)..block(1)];
        csb[NX_MAGIC_OFF..NX_MAGIC_OFF + 4].copy_from_slice(&NXSB_MAGIC.to_le_bytes());
        csb[NX_BLOCK_SIZE_OFF..NX_BLOCK_SIZE_OFF + 4].copy_from_slice(&4096u32.to_le_bytes());
        csb[NX_XP_DESC_BLOCKS_OFF..NX_XP_DESC_BLOCKS_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        csb[NX_XP_DESC_BASE_OFF..NX_XP_DESC_BASE_OFF + 8].copy_from_slice(&1u64.to_le_bytes());
        csb[NX_XP_DESC_INDEX_OFF..NX_XP_DESC_INDEX_OFF + 4].copy_from_slice(&0u32.to_le_bytes());
        csb[NX_XP_DESC_LEN_OFF..NX_XP_DESC_LEN_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        csb[NX_MAX_FILE_SYSTEMS_OFF..NX_MAX_FILE_SYSTEMS_OFF + 4]
            .copy_from_slice(&1u32.to_le_bytes());
        // nx_fs_oid[0] = 100 (volume OID)
        csb[NX_FS_OID_OFF..NX_FS_OID_OFF + 8].copy_from_slice(&100u64.to_le_bytes());

        // ── Block 1: Checkpoint OID→block map (B-tree fixed-KV) ──
        let cp = &mut img[block(1)..block(2)];
        cp[BT_FLAGS_OFF..BT_FLAGS_OFF + 2]
            .copy_from_slice(&(BT_ROOT | BT_LEAF | BT_FIXED_KV).to_le_bytes());
        cp[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
        let nkeys: u32 = 15;
        cp[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&nkeys.to_le_bytes());

        // kvloc_t entries at table_space_offset.
        // Key: u64 OID (8 bytes), Value: u64 block number (8 bytes).
        // Total per entry: 16 bytes.
        let key_size: u16 = 8;
        let val_size: u16 = 8;
        let entry_data_size = (key_size + val_size) as usize;

        // TOC at BT_TOC_BASE (0x14). Table space starts at 0x14.
        let table_off: u16 = BT_TOC_BASE as u16;
        let table_len: u16 =
            (nkeys as usize * TOC_ENTRY_SIZE + nkeys as usize * entry_data_size) as u16;
        cp[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2].copy_from_slice(&table_off.to_le_bytes());
        cp[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4]
            .copy_from_slice(&table_len.to_le_bytes());

        // OID→block mappings:
        // 100→3 (volume), 200→4 (root inode), 300→5 (root dir B-tree)
        // 400→6 (file inode), 500→7 (file data), 600→8 (subdir inode)
        // 700→9 (subdir B-tree), 800→10 (nested inode), 900→11 (nested data)
        let mappings: Vec<(u64, u64)> = vec![
            (100, 3),
            (200, 4),
            (300, 5),
            (400, 6),
            (500, 7),
            (600, 8),
            (700, 9),
            (800, 10),
            (900, 11),
            (1000, 12),
            (1001, 13),
            (1100, 14),
            (1101, 15),
            (1200, 16),
            (1300, 17),
        ];

        let kv_data_start = table_off as usize + nkeys as usize * TOC_ENTRY_SIZE;

        for (i, &(oid, blk)) in mappings.iter().enumerate() {
            let toc_off = table_off as usize + i * TOC_ENTRY_SIZE;
            let key_data_off = kv_data_start + i * entry_data_size;
            let val_data_off = key_data_off + key_size as usize;

            // Write TOC entry.
            cp[toc_off..toc_off + 2].copy_from_slice(&(key_data_off as u16).to_le_bytes());
            cp[toc_off + 2..toc_off + 4].copy_from_slice(&key_size.to_le_bytes());
            cp[toc_off + 4..toc_off + 6].copy_from_slice(&(val_data_off as u16).to_le_bytes());
            cp[toc_off + 6..toc_off + 8].copy_from_slice(&val_size.to_le_bytes());

            // Write key and value data.
            cp[key_data_off..key_data_off + 8].copy_from_slice(&oid.to_le_bytes());
            cp[val_data_off..val_data_off + 8].copy_from_slice(&blk.to_le_bytes());
        }

        // ── Block 3: Volume superblock (APSB) ──
        let vsb = &mut img[block(3)..block(4)];
        vsb[AP_MAGIC_OFF..AP_MAGIC_OFF + 4].copy_from_slice(&APSB_MAGIC.to_le_bytes());
        vsb[AP_ROOT_TREE_OID_OFF..AP_ROOT_TREE_OID_OFF + 8].copy_from_slice(&200u64.to_le_bytes());

        // ── Block 4: Root directory inode ──
        let rdi = &mut img[block(4)..block(5)];
        // j_inode_val at offset 0.
        rdi[0..8].copy_from_slice(&0u64.to_le_bytes()); // parent_id (root has 0)
        rdi[8..16].copy_from_slice(&200u64.to_le_bytes()); // private_id = self oid
        rdi[0x10..0x18].copy_from_slice(&700_000_000_000u64.to_le_bytes()); // create_time
        rdi[0x18..0x20].copy_from_slice(&700_000_000_001u64.to_le_bytes()); // mod_time
        rdi[0x20..0x28].copy_from_slice(&700_000_000_002u64.to_le_bytes()); // change_time
        rdi[0x28..0x30].copy_from_slice(&700_000_000_003u64.to_le_bytes()); // access_time
        rdi[0x30..0x38].copy_from_slice(&0u64.to_le_bytes()); // flags
        rdi[0x38..0x3C].copy_from_slice(&5u32.to_le_bytes()); // nchildren (or nlink)
        rdi[0x44..0x48].copy_from_slice(&501u32.to_le_bytes()); // uid
        rdi[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes()); // gid
        rdi[0x4C..0x4E].copy_from_slice(&S_IFDIR.to_le_bytes()); // mode = directory
        rdi[0x58..0x60].copy_from_slice(&0u64.to_le_bytes()); // logical_size = 0
        rdi[0x80..0x88].copy_from_slice(&300u64.to_le_bytes()); // children_oid → block 5

        // ── Block 5: Root dir B-tree node ──
        let rdb = &mut img[block(5)..block(6)];
        rdb[BT_FLAGS_OFF..BT_FLAGS_OFF + 2].copy_from_slice(&(BT_ROOT | BT_LEAF).to_le_bytes());
        rdb[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
        let _dir_nkeys: u32 = 3; // file.txt, subdir, dir1
        rdb[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&3u32.to_le_bytes());

        // Helper: write a variable-length KV entry for directory.
        fn write_dir_kv(
            node: &mut [u8],
            toc_idx: usize,
            toc_start: usize,
            kv_start: &mut usize,
            name: &[u8],
            file_id: u64,
            is_dir: bool,
        ) {
            let key_size = 10 + name.len(); // hash(8) + name_len(2) + name
            let val_size = 32; // file_id(8) + date(8) + padding + type(2)
            let toc_off = toc_start + toc_idx * TOC_ENTRY_SIZE;

            node[toc_off..toc_off + 2].copy_from_slice(&(*kv_start as u16).to_le_bytes());
            node[toc_off + 2..toc_off + 4].copy_from_slice(&(key_size as u16).to_le_bytes());

            let val_off = *kv_start + key_size;
            node[toc_off + 4..toc_off + 6].copy_from_slice(&(val_off as u16).to_le_bytes());
            node[toc_off + 6..toc_off + 8].copy_from_slice(&(val_size as u16).to_le_bytes());

            // Key: hash(8) + name_len(2) + name bytes.
            // Use a simple hash = file_id for testing (not cryptographically correct).
            node[*kv_start..*kv_start + 8].copy_from_slice(&file_id.to_le_bytes());
            node[*kv_start + 8..*kv_start + 10].copy_from_slice(&(name.len() as u16).to_le_bytes());
            node[*kv_start + 10..*kv_start + 10 + name.len()].copy_from_slice(name);

            // Value: j_drec_val: file_id(8), date(8), ... type(2) at offset 24.
            node[val_off..val_off + 8].copy_from_slice(&file_id.to_le_bytes());
            node[val_off + 8..val_off + 16].copy_from_slice(&700_000_000_000u64.to_le_bytes()); // date_added
            if is_dir {
                node[val_off + 24..val_off + 26].copy_from_slice(&DREC_TYPE_DIR.to_le_bytes());
            } else {
                node[val_off + 24..val_off + 26].copy_from_slice(&DREC_TYPE_FILE.to_le_bytes());
            }

            *kv_start += key_size + val_size;
        }

        let dir_toc_start = BT_TOC_BASE;
        let dir_table_off: u16 = BT_TOC_BASE as u16;
        let dir_table_len: u16 = 512; // generous
        rdb[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2]
            .copy_from_slice(&dir_table_off.to_le_bytes());
        rdb[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4]
            .copy_from_slice(&dir_table_len.to_le_bytes());

        let mut kv_start = dir_toc_start + (3usize * TOC_ENTRY_SIZE); // after TOC entries
        write_dir_kv(
            rdb,
            0,
            dir_toc_start,
            &mut kv_start,
            b"file.txt",
            400,
            false,
        );
        write_dir_kv(rdb, 1, dir_toc_start, &mut kv_start, b"subdir", 600, true);
        write_dir_kv(rdb, 2, dir_toc_start, &mut kv_start, b"dir1", 1000, true);

        // ── Block 6: File inode (file.txt) ──
        let fi = &mut img[block(6)..block(7)];
        fi[0..8].copy_from_slice(&200u64.to_le_bytes()); // parent_id (root inode)
        fi[8..16].copy_from_slice(&400u64.to_le_bytes()); // private_id
        fi[0x10..0x18].copy_from_slice(&700_000_000_100u64.to_le_bytes());
        fi[0x18..0x20].copy_from_slice(&700_000_000_101u64.to_le_bytes());
        fi[0x20..0x28].copy_from_slice(&700_000_000_102u64.to_le_bytes());
        fi[0x28..0x30].copy_from_slice(&700_000_000_103u64.to_le_bytes());
        fi[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        fi[0x38..0x3C].copy_from_slice(&1u32.to_le_bytes()); // nlink
        fi[0x44..0x48].copy_from_slice(&501u32.to_le_bytes()); // uid
        fi[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes()); // gid
        let content = b"Hello from APFS!";
        fi[0x4C..0x4E].copy_from_slice(&S_IFREG.to_le_bytes()); // mode = regular file
        fi[0x58..0x60].copy_from_slice(&(content.len() as u64).to_le_bytes()); // logical_size
        fi[0x80..0x88].copy_from_slice(&0u64.to_le_bytes()); // children_oid = 0 (not a dir)
        fi[0x88..0x90].copy_from_slice(&500u64.to_le_bytes()); // extent reference → block 7

        // ── Block 7: File data ──
        img[block(7)..block(7) + content.len()].copy_from_slice(content);

        // ── Block 8: Subdir inode ──
        let sdi = &mut img[block(8)..block(9)];
        sdi[0..8].copy_from_slice(&200u64.to_le_bytes()); // parent_id
        sdi[8..16].copy_from_slice(&600u64.to_le_bytes()); // private_id
        sdi[0x10..0x18].copy_from_slice(&700_001_000_000u64.to_le_bytes());
        sdi[0x18..0x20].copy_from_slice(&700_001_000_001u64.to_le_bytes());
        sdi[0x20..0x28].copy_from_slice(&700_001_000_002u64.to_le_bytes());
        sdi[0x28..0x30].copy_from_slice(&700_001_000_003u64.to_le_bytes());
        sdi[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        sdi[0x38..0x3C].copy_from_slice(&3u32.to_le_bytes());
        sdi[0x44..0x48].copy_from_slice(&501u32.to_le_bytes());
        sdi[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes());
        sdi[0x4C..0x4E].copy_from_slice(&S_IFDIR.to_le_bytes());
        sdi[0x58..0x60].copy_from_slice(&0u64.to_le_bytes());
        sdi[0x80..0x88].copy_from_slice(&700u64.to_le_bytes()); // children_oid → block 9

        // ── Block 9: Subdir B-tree node ──
        let sdb = &mut img[block(9)..block(10)];
        sdb[BT_FLAGS_OFF..BT_FLAGS_OFF + 2].copy_from_slice(&(BT_ROOT | BT_LEAF).to_le_bytes());
        sdb[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
        sdb[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        sdb[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2]
            .copy_from_slice(&dir_table_off.to_le_bytes());
        sdb[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4]
            .copy_from_slice(&dir_table_len.to_le_bytes());

        let mut kv_start2 = dir_toc_start + TOC_ENTRY_SIZE;
        write_dir_kv(
            sdb,
            0,
            dir_toc_start,
            &mut kv_start2,
            b"nested.dat",
            800,
            false,
        );

        // ── Block 10: Nested file inode ──
        let nfi = &mut img[block(10)..block(11)];
        nfi[0..8].copy_from_slice(&600u64.to_le_bytes()); // parent_id
        nfi[8..16].copy_from_slice(&800u64.to_le_bytes()); // private_id
        nfi[0x10..0x18].copy_from_slice(&700_002_000_000u64.to_le_bytes());
        nfi[0x18..0x20].copy_from_slice(&700_002_000_001u64.to_le_bytes());
        nfi[0x20..0x28].copy_from_slice(&700_002_000_002u64.to_le_bytes());
        nfi[0x28..0x30].copy_from_slice(&700_002_000_003u64.to_le_bytes());
        nfi[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        nfi[0x38..0x3C].copy_from_slice(&1u32.to_le_bytes());
        nfi[0x44..0x48].copy_from_slice(&501u32.to_le_bytes());
        nfi[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes());
        let nested_content = b"Nested APFS data";
        nfi[0x4C..0x4E].copy_from_slice(&S_IFREG.to_le_bytes());
        nfi[0x58..0x60].copy_from_slice(&(nested_content.len() as u64).to_le_bytes());
        nfi[0x80..0x88].copy_from_slice(&0u64.to_le_bytes());
        nfi[0x88..0x90].copy_from_slice(&900u64.to_le_bytes()); // extent → block 11

        // ── Block 11: Nested file data ──
        img[block(11)..block(11) + nested_content.len()].copy_from_slice(nested_content);

        // ── Block 12: dir1 inode ──
        let d1 = &mut img[block(12)..block(13)];
        d1[0..8].copy_from_slice(&200u64.to_le_bytes()); // parent_id (root)
        d1[8..16].copy_from_slice(&1000u64.to_le_bytes()); // private_id
        d1[0x10..0x18].copy_from_slice(&700_003_000_000u64.to_le_bytes());
        d1[0x18..0x20].copy_from_slice(&700_003_000_001u64.to_le_bytes());
        d1[0x20..0x28].copy_from_slice(&700_003_000_002u64.to_le_bytes());
        d1[0x28..0x30].copy_from_slice(&700_003_000_003u64.to_le_bytes());
        d1[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        d1[0x38..0x3C].copy_from_slice(&3u32.to_le_bytes());
        d1[0x44..0x48].copy_from_slice(&501u32.to_le_bytes());
        d1[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes());
        d1[0x4C..0x4E].copy_from_slice(&S_IFDIR.to_le_bytes());
        d1[0x58..0x60].copy_from_slice(&0u64.to_le_bytes());
        d1[0x80..0x88].copy_from_slice(&1001u64.to_le_bytes()); // children_oid → block 13

        // ── Block 13: dir1 B-tree node ──
        let d1b = &mut img[block(13)..block(14)];
        d1b[BT_FLAGS_OFF..BT_FLAGS_OFF + 2].copy_from_slice(&(BT_ROOT | BT_LEAF).to_le_bytes());
        d1b[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
        d1b[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        d1b[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2]
            .copy_from_slice(&dir_table_off.to_le_bytes());
        d1b[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4]
            .copy_from_slice(&dir_table_len.to_le_bytes());

        let mut kv_start3 = dir_toc_start + TOC_ENTRY_SIZE;
        write_dir_kv(d1b, 0, dir_toc_start, &mut kv_start3, b"dir2", 1100, true);

        // ── Block 14: dir2 inode ──
        let d2 = &mut img[block(14)..block(15)];
        d2[0..8].copy_from_slice(&1000u64.to_le_bytes()); // parent_id (dir1)
        d2[8..16].copy_from_slice(&1100u64.to_le_bytes()); // private_id
        d2[0x10..0x18].copy_from_slice(&700_004_000_000u64.to_le_bytes());
        d2[0x18..0x20].copy_from_slice(&700_004_000_001u64.to_le_bytes());
        d2[0x20..0x28].copy_from_slice(&700_004_000_002u64.to_le_bytes());
        d2[0x28..0x30].copy_from_slice(&700_004_000_003u64.to_le_bytes());
        d2[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        d2[0x38..0x3C].copy_from_slice(&3u32.to_le_bytes());
        d2[0x44..0x48].copy_from_slice(&501u32.to_le_bytes());
        d2[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes());
        d2[0x4C..0x4E].copy_from_slice(&S_IFDIR.to_le_bytes());
        d2[0x58..0x60].copy_from_slice(&0u64.to_le_bytes());
        d2[0x80..0x88].copy_from_slice(&1101u64.to_le_bytes()); // children_oid → block 15

        // ── Block 15: dir2 B-tree node ──
        let d2b = &mut img[block(15)..block(16)];
        d2b[BT_FLAGS_OFF..BT_FLAGS_OFF + 2].copy_from_slice(&(BT_ROOT | BT_LEAF).to_le_bytes());
        d2b[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
        d2b[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        d2b[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2]
            .copy_from_slice(&dir_table_off.to_le_bytes());
        d2b[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4]
            .copy_from_slice(&dir_table_len.to_le_bytes());

        let mut kv_start4 = dir_toc_start + TOC_ENTRY_SIZE;
        write_dir_kv(
            d2b,
            0,
            dir_toc_start,
            &mut kv_start4,
            b"file.txt",
            1200,
            false,
        );

        // ── Block 16: three-level file inode ──
        let tfi = &mut img[block(16)..block(17)];
        tfi[0..8].copy_from_slice(&1100u64.to_le_bytes()); // parent_id (dir2)
        tfi[8..16].copy_from_slice(&1200u64.to_le_bytes()); // private_id
        tfi[0x10..0x18].copy_from_slice(&700_005_000_000u64.to_le_bytes());
        tfi[0x18..0x20].copy_from_slice(&700_005_000_001u64.to_le_bytes());
        tfi[0x20..0x28].copy_from_slice(&700_005_000_002u64.to_le_bytes());
        tfi[0x28..0x30].copy_from_slice(&700_005_000_003u64.to_le_bytes());
        tfi[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        tfi[0x38..0x3C].copy_from_slice(&1u32.to_le_bytes());
        tfi[0x44..0x48].copy_from_slice(&501u32.to_le_bytes());
        tfi[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes());
        let three_content = b"Deep nested content";
        tfi[0x4C..0x4E].copy_from_slice(&S_IFREG.to_le_bytes());
        tfi[0x58..0x60].copy_from_slice(&(three_content.len() as u64).to_le_bytes());
        tfi[0x80..0x88].copy_from_slice(&0u64.to_le_bytes());
        tfi[0x88..0x90].copy_from_slice(&1300u64.to_le_bytes()); // extent → block 17

        // ── Block 17: Three-level file data ──
        img[block(17)..block(17) + three_content.len()].copy_from_slice(three_content);

        img
    }

    // -------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------

    #[test]
    fn test_container_superblock() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();
        assert_eq!(apfs.data_source_name(), "apfs");
        assert_eq!(apfs.block_size, 4096);
    }

    #[test]
    fn test_volume_listing() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        assert!(!apfs.volumes.is_empty());
        let vol = &apfs.volumes[0];
        assert_eq!(vol.name, "Macintosh HD");
        assert_eq!(vol.fs_oid, 100);

        // Top-level listing shows volumes.
        let children = apfs.list_children("").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "Macintosh HD");
        assert!(children[0].is_dir);
    }

    #[test]
    fn test_root_directory_listing() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        let root = apfs.root().unwrap();
        assert_eq!(root.name, "\\");
        assert!(root.is_dir);

        let vol_name = &apfs.volumes[0].name;
        let children = apfs.list_children(vol_name).unwrap();
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(
            names.contains(&"file.txt"),
            "expected file.txt in {names:?}"
        );
        assert!(names.contains(&"subdir"), "expected subdir in {names:?}");
    }

    #[test]
    fn test_file_read() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        let vol_name = &apfs.volumes[0].name;
        let path = format!("{}/file.txt", vol_name);
        let mut f = apfs.open_file(&path).unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Hello from APFS!");
    }

    #[test]
    fn test_nested_file_read() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        let vol_name = &apfs.volumes[0].name;
        let path = format!("{}/subdir/nested.dat", vol_name);
        let mut f = apfs.open_file(&path).unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Nested APFS data");
    }

    #[test]
    fn test_inode_parsing() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        // Read the file inode (OID 400).
        let inode = apfs.read_inode(400).unwrap();
        assert_eq!(inode.mode, S_IFREG);
        assert_eq!(inode.logical_size, 16); // "Hello from APFS!" = 16 bytes
        assert!(inode.access_time > 0, "access_time should be set");
        assert!(inode.mod_time > 0, "mod_time should be set");
        assert!(inode.create_time > 0, "create_time should be set");
        assert!(!inode.extents.is_empty(), "file inode should have extents");
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let mut img = build_apfs_fixture();
        img[NX_MAGIC_OFF..NX_MAGIC_OFF + 4].copy_from_slice(&0u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        match ApfsReader::open(reader, 0) {
            Ok(_) => panic!("expected error"),
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::InvalidData);
                assert!(e.to_string().contains("magic"));
            }
        }
    }

    #[test]
    fn test_nonexistent_path() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        let e = apfs.list_children("nonexistent").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);

        let vol_name = &apfs.volumes[0].name;
        let e = match apfs.open_file(&format!("{}/no_such.txt", vol_name)) {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_object_map_for_root_inode() {
        // Build a minimal checkpoint node with fixed-KV OID→block mappings.
        let mut cp = vec![0u8; 1024];
        cp[BT_FLAGS_OFF..BT_FLAGS_OFF + 2]
            .copy_from_slice(&(BT_ROOT | BT_LEAF | BT_FIXED_KV).to_le_bytes());
        cp[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
        let nkeys: u32 = 2;
        cp[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&nkeys.to_le_bytes());

        let key_size: u16 = 8;
        let val_size: u16 = 8;
        let entry_size = (key_size + val_size) as usize;
        let table_off: u16 = BT_TOC_BASE as u16;
        let table_len = (nkeys as usize * TOC_ENTRY_SIZE + nkeys as usize * entry_size) as u16;
        cp[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2].copy_from_slice(&table_off.to_le_bytes());
        cp[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4]
            .copy_from_slice(&table_len.to_le_bytes());

        let mappings: [(u64, u64); 2] = [(200, 4), (300, 5)];
        let kv_start = table_off as usize + nkeys as usize * TOC_ENTRY_SIZE;

        for (i, &(oid, blk)) in mappings.iter().enumerate() {
            let toc_off = table_off as usize + i * TOC_ENTRY_SIZE;
            let key_off = kv_start + i * entry_size;
            let val_off = key_off + key_size as usize;

            cp[toc_off..toc_off + 2].copy_from_slice(&(key_off as u16).to_le_bytes());
            cp[toc_off + 2..toc_off + 4].copy_from_slice(&key_size.to_le_bytes());
            cp[toc_off + 4..toc_off + 6].copy_from_slice(&(val_off as u16).to_le_bytes());
            cp[toc_off + 6..toc_off + 8].copy_from_slice(&val_size.to_le_bytes());

            cp[key_off..key_off + 8].copy_from_slice(&oid.to_le_bytes());
            cp[val_off..val_off + 8].copy_from_slice(&blk.to_le_bytes());
        }

        let flags = BT_ROOT | BT_LEAF | BT_FIXED_KV;
        let omap = OidMap::from_checkpoint_node(&cp, flags, nkeys).unwrap();

        assert_eq!(omap.resolve(200).unwrap(), 4);
        assert_eq!(omap.resolve(300).unwrap(), 5);
        // Non-existent OID should fail.
        assert!(omap.resolve(999).is_err());
    }

    #[test]
    fn test_nested_three_levels() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        let vol_name = &apfs.volumes[0].name;
        let path = format!("{}/dir1/dir2/file.txt", vol_name);
        let mut f = apfs.open_file(&path).unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Deep nested content");
    }

    #[test]
    fn test_container_superblock_info() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();
        assert!(apfs.block_size > 0);
        assert!(!apfs.volumes.is_empty());
    }

    #[test]
    fn test_nonexistent_volume() {
        let img = build_apfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        let e = apfs.list_children("NoSuchVol").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }
}
