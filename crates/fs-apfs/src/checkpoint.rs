//! APFS checkpoint rewind, diff, and deleted-file recovery.
//!
//! APFS containers store OID→block translation in a checkpoint descriptor area
//! that can hold multiple checkpoint states as the filesystem evolves.  This
//! module reads the checkpoint descriptor metadata from the container superblock
//! and exposes higher-level operations on top of `ApfsReader`.

use crate::{
    ns_to_option_dt, parse_dir_b_tree, parse_inode_val, ApfsInode, ApfsReader, ApfsVolume,
    DirEntry, OidMap, BT_FLAGS_OFF, BT_LEAF, BT_NKEYS_OFF, NX_XP_DESC_BASE_OFF,
    NX_XP_DESC_BLOCKS_OFF, NX_XP_DESC_INDEX_OFF, NX_XP_DESC_LEN_OFF,
};
use evidence_core::filesystem::{fs_node, invalid_fs_data, FsNode};
use std::collections::HashMap;
use std::io;

// ---------------------------------------------------------------------------
// Offsets / constants
// ---------------------------------------------------------------------------

/// nx_xp_desc_next in the container superblock (u32 LE).
const NX_XP_DESC_NEXT_OFF: usize = 0x60;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Metadata extracted from the container superblock about the checkpoint
/// descriptor area.
#[derive(Debug, Clone)]
pub struct CheckpointDescriptor {
    /// First block of the checkpoint descriptor area (logical block number).
    pub base_block: u64,
    /// Number of blocks in the descriptor area.
    pub block_count: u32,
    /// Offset in blocks from `base_block` to the currently active checkpoint.
    pub active_index: u32,
    /// Length in blocks of a single checkpoint descriptor.
    pub data_length: u32,
    /// Next free index for a new checkpoint descriptor (offset in blocks).
    pub next_index: u32,
}

/// Difference between two APFS checkpoint states.
#[derive(Debug, Clone)]
pub struct CheckpointDiff {
    /// Nodes present in `cp2` but absent in `cp1`.
    pub added: Vec<FsNode>,
    /// Nodes present in `cp1` but absent in `cp2`.
    pub removed: Vec<FsNode>,
    /// Nodes present in both but with differing size or timestamps.
    pub changed: Vec<FsNodeChange>,
}

/// A single changed file entry between two checkpoints.
#[derive(Debug, Clone)]
pub struct FsNodeChange {
    pub path: String,
    pub old_size: u64,
    pub new_size: u64,
    /// `old_modified` is the mtime from `cp1`; `new_modified` is from `cp2`.
    pub old_modified: Option<chrono::DateTime<chrono::Utc>>,
    pub new_modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// A file recovered from an older checkpoint that no longer appears in
/// the current active checkpoint.
#[derive(Debug, Clone)]
pub struct RecoveredFile {
    pub path: String,
    pub node: FsNode,
    /// Index of the checkpoint (offset in blocks within the checkpoint area)
    /// where this file was found.
    pub checkpoint_index: u32,
}

// ---------------------------------------------------------------------------
// ApfsReader checkpoint methods
// ---------------------------------------------------------------------------

impl ApfsReader {
    // -- public checkpoint API ------------------------------------------------

    /// Parse the checkpoint descriptor metadata from the container superblock.
    ///
    /// This re-reads the superblock at `volume_offset` to extract the current
    /// values of `nx_xp_desc_*` fields.
    pub fn parse_checkpoint_descriptor(&self) -> io::Result<CheckpointDescriptor> {
        let mut reader = self.reader.borrow_mut();
        reader.seek(std::io::SeekFrom::Start(self.volume_offset))?;
        let mut block0 = [0u8; 4096];
        reader.read_exact(&mut block0)?;

        let block_count = u32::from_le_bytes(
            block0[NX_XP_DESC_BLOCKS_OFF..NX_XP_DESC_BLOCKS_OFF + 4]
                .try_into()
                .unwrap(),
        );
        let base_block = u64::from_le_bytes(
            block0[NX_XP_DESC_BASE_OFF..NX_XP_DESC_BASE_OFF + 8]
                .try_into()
                .unwrap(),
        );
        let active_index = u32::from_le_bytes(
            block0[NX_XP_DESC_INDEX_OFF..NX_XP_DESC_INDEX_OFF + 4]
                .try_into()
                .unwrap(),
        );
        let data_length = u32::from_le_bytes(
            block0[NX_XP_DESC_LEN_OFF..NX_XP_DESC_LEN_OFF + 4]
                .try_into()
                .unwrap(),
        );
        let next_index = if NX_XP_DESC_NEXT_OFF + 4 <= block0.len() {
            u32::from_le_bytes(
                block0[NX_XP_DESC_NEXT_OFF..NX_XP_DESC_NEXT_OFF + 4]
                    .try_into()
                    .unwrap(),
            )
        } else {
            0
        };

        Ok(CheckpointDescriptor {
            base_block,
            block_count,
            active_index,
            data_length,
            next_index,
        })
    }

    /// Rewind the filesystem view to a previous checkpoint and return the
    /// full file tree as it existed at that point.
    ///
    /// `checkpoint_index` is the offset in blocks from the checkpoint area
    /// base (0 = oldest available, `active_index` = current).
    pub fn rewind_to_checkpoint(&self, checkpoint_index: u32) -> io::Result<Vec<FsNode>> {
        let oid_map = self.read_checkpoint_oid_map(checkpoint_index)?;

        let mut all_nodes = Vec::new();
        for vol in &self.volumes {
            let vol_nodes = self.list_volume_all_files(&oid_map, vol)?;
            // Prefix with volume name to match the reader's path convention.
            for node in vol_nodes {
                let mut prefixed = node;
                prefixed.path = format!("{}/{}", vol.name, prefixed.path);
                all_nodes.push(prefixed);
            }
        }
        Ok(all_nodes)
    }

    /// Compute the difference between two checkpoint states.
    ///
    /// `cp1` and `cp2` are checkpoint indices (offsets within the checkpoint
    /// area).  Returns a `CheckpointDiff` describing what was added, removed,
    /// and changed between the two states (cp1 → cp2).
    pub fn diff_checkpoints(&self, cp1: u32, cp2: u32) -> io::Result<CheckpointDiff> {
        let files1 = self.rewind_to_checkpoint(cp1)?;
        let files2 = self.rewind_to_checkpoint(cp2)?;

        Ok(diff_file_lists(&files1, &files2))
    }

    /// Find files that are present in any older checkpoint but absent from
    /// the current active checkpoint.
    ///
    /// This scans checkpoint indices from 0 up to (but not including) the
    /// active index and reports files that exist only in those older states.
    pub fn recover_deleted_files(&self) -> io::Result<Vec<RecoveredFile>> {
        let desc = self.parse_checkpoint_descriptor()?;
        let active = desc.active_index;

        let current_oid_map = self.read_checkpoint_oid_map(active)?;
        let mut current_files: HashMap<String, FsNode> = HashMap::new();
        for vol in &self.volumes {
            let nodes = self.list_volume_all_files(&current_oid_map, vol)?;
            for node in nodes {
                current_files.insert(format!("{}/{}", vol.name, node.path), node);
            }
        }

        let mut recovered = Vec::new();
        for idx in 0..active {
            let old_oid_map = self.read_checkpoint_oid_map(idx)?;
            let mut old_files: HashMap<String, FsNode> = HashMap::new();
            for vol in &self.volumes {
                let nodes = self.list_volume_all_files(&old_oid_map, vol)?;
                for node in nodes {
                    old_files.insert(format!("{}/{}", vol.name, node.path), node);
                }
            }

            for (path, node) in &old_files {
                if !current_files.contains_key(path) {
                    recovered.push(RecoveredFile {
                        path: path.clone(),
                        node: node.clone(),
                        checkpoint_index: idx,
                    });
                }
            }
        }

        recovered.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(recovered)
    }

    // -- internal helpers ----------------------------------------------------

    /// Read a specific checkpoint descriptor block from the checkpoint area
    /// and parse its OID map.
    fn read_checkpoint_oid_map(&self, checkpoint_index: u32) -> io::Result<OidMap> {
        let desc = self.parse_checkpoint_descriptor()?;
        let cp_block = desc.base_block + checkpoint_index as u64;

        let mut reader = self.reader.borrow_mut();
        let offset = self.volume_offset + cp_block * self.block_size as u64;
        let mut buf = vec![0u8; self.block_size as usize];
        reader.seek(std::io::SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;

        let flags = u16::from_le_bytes(
            buf[BT_FLAGS_OFF..BT_FLAGS_OFF + 2]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let nkeys = u32::from_le_bytes(
            buf[BT_NKEYS_OFF..BT_NKEYS_OFF + 4]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );

        OidMap::from_checkpoint_node(&buf, flags, nkeys)
    }

    /// Recursively list all files and directories under a volume root using
    /// the supplied OID map.
    fn list_volume_all_files(&self, oid_map: &OidMap, vol: &ApfsVolume) -> io::Result<Vec<FsNode>> {
        self.list_all_files_recursive(oid_map, vol.root_tree_oid, "")
    }

    fn list_all_files_recursive(
        &self,
        oid_map: &OidMap,
        inode_oid: u64,
        parent_path: &str,
    ) -> io::Result<Vec<FsNode>> {
        let inode = self.read_inode_with_map(oid_map, inode_oid)?;
        if inode.children_oid == 0 {
            return Ok(Vec::new());
        }

        let entries = self.list_directory_with_map(oid_map, &inode)?;
        let mut nodes = Vec::new();

        for entry in entries {
            let child_path = if parent_path.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", parent_path, entry.name)
            };

            // If the OID cannot be resolved in this checkpoint's map,
            // the file is not accessible — skip it (it was "deleted").
            let child_inode = match self.read_inode_with_map(oid_map, entry.file_id) {
                Ok(inode) => inode,
                Err(_) => continue,
            };

            let node = fs_node(
                entry.name.clone(),
                entry.is_dir,
                child_inode.logical_size,
                ns_to_option_dt(child_inode.create_time),
                ns_to_option_dt(child_inode.mod_time),
                ns_to_option_dt(child_inode.access_time),
            );
            // Set the path relative to volume root.
            let mut pathful = node;
            pathful.path = child_path.clone();
            nodes.push(pathful);

            if entry.is_dir {
                let children =
                    self.list_all_files_recursive(oid_map, entry.file_id, &child_path)?;
                nodes.extend(children);
            }
        }

        Ok(nodes)
    }

    // -- OID map-aware traversal helpers -------------------------------------

    fn resolve_oid_block_with_map(&self, oid_map: &OidMap, oid: u64) -> io::Result<Vec<u8>> {
        let block_no = oid_map.resolve(oid)?;
        self.read_block(block_no)
    }

    fn read_btree_node_with_map(
        &self,
        oid_map: &OidMap,
        oid: u64,
    ) -> io::Result<(Vec<u8>, u16, u32)> {
        let data = self.resolve_oid_block_with_map(oid_map, oid)?;
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

        if flags & BT_LEAF == 0 {
            let toc = crate::parse_toc(&data, nkeys);
            for entry in &toc {
                let val_start = entry.val_off as usize;
                if val_start + 8 <= data.len() {
                    let child_oid = u64::from_le_bytes(
                        data[val_start..val_start + 8]
                            .try_into()
                            .map_err(|_| invalid_fs_data("disk parse error"))?,
                    );
                    if child_oid != 0 {
                        let child_data = self.resolve_oid_block_with_map(oid_map, child_oid)?;
                        let cf = u16::from_le_bytes(
                            child_data[BT_FLAGS_OFF..BT_FLAGS_OFF + 2]
                                .try_into()
                                .unwrap(),
                        );
                        if cf & BT_LEAF != 0 {
                            let cn = u32::from_le_bytes(
                                child_data[BT_NKEYS_OFF..BT_NKEYS_OFF + 4]
                                    .try_into()
                                    .unwrap(),
                            );
                            return Ok((child_data, cf, cn));
                        }
                    }
                }
            }
        }

        Ok((data, flags, nkeys))
    }

    fn read_inode_with_map(&self, oid_map: &OidMap, oid: u64) -> io::Result<ApfsInode> {
        let data = self.resolve_oid_block_with_map(oid_map, oid)?;
        parse_inode_val(&data, oid)
    }

    fn list_directory_with_map(
        &self,
        oid_map: &OidMap,
        dir_inode: &ApfsInode,
    ) -> io::Result<Vec<DirEntry>> {
        if dir_inode.children_oid == 0 {
            return Ok(Vec::new());
        }
        let (node_data, flags, nkeys) =
            self.read_btree_node_with_map(oid_map, dir_inode.children_oid)?;
        parse_dir_b_tree(&node_data, flags, nkeys)
    }
}

// ---------------------------------------------------------------------------
// Diff helper (standalone, used by checkpoint and test code)
// ---------------------------------------------------------------------------

/// Compute a diff between two flat file lists keyed by path.
pub(crate) fn diff_file_lists(files1: &[FsNode], files2: &[FsNode]) -> CheckpointDiff {
    let map1: HashMap<&str, &FsNode> = files1.iter().map(|n| (n.path.as_str(), n)).collect();
    let map2: HashMap<&str, &FsNode> = files2.iter().map(|n| (n.path.as_str(), n)).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    // Files in map2 not in map1 → added.
    for (&path, &node) in &map2 {
        if !map1.contains_key(path) {
            added.push(node.clone());
        }
    }

    // Files in map1 not in map2 → removed.
    for (&path, &node) in &map1 {
        if !map2.contains_key(path) {
            removed.push(node.clone());
        }
    }

    // Files in both with different size or mtime → changed.
    for (&path, &node1) in &map1 {
        if let Some(&node2) = map2.get(path) {
            if node1.size != node2.size || node1.modified_at != node2.modified_at {
                changed.push(FsNodeChange {
                    path: path.to_string(),
                    old_size: node1.size,
                    new_size: node2.size,
                    old_modified: node1.modified_at,
                    new_modified: node2.modified_at,
                });
            }
        }
    }

    added.sort_by(|a, b| a.path.cmp(&b.path));
    removed.sort_by(|a, b| a.path.cmp(&b.path));
    changed.sort_by(|a, b| a.path.cmp(&b.path));

    CheckpointDiff {
        added,
        removed,
        changed,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FakeReader, APSB_MAGIC, AP_MAGIC_OFF, AP_ROOT_TREE_OID_OFF, BT_FIXED_KV, BT_LEVEL_OFF,
        BT_ROOT, BT_TABLE_SPACE_OFF, BT_TOC_BASE, DREC_TYPE_DIR, DREC_TYPE_FILE, NXSB_MAGIC,
        NX_BLOCK_SIZE_OFF, NX_FS_OID_OFF, NX_MAGIC_OFF, NX_MAX_FILE_SYSTEMS_OFF, S_IFDIR, S_IFREG,
        TOC_ENTRY_SIZE,
    };
    use evidence_core::EvidenceReader;

    /// Build a fixture with two checkpoint descriptors so we can exercise
    /// rewind / diff / recovery.
    ///
    /// Layout (block_size = 4096, 14 blocks = 0xE000 bytes):
    ///
    ///  Block  Offset    Content
    ///  -----  --------  ----------------------------------------------
    ///    0    0x00000   Container superblock (NXSB)
    ///    1    0x01000   Checkpoint [0] — older (includes deleted file)
    ///    2    0x02000   Checkpoint [1] — current / active
    ///    3    0x03000   Volume superblock (APSB)
    ///    4    0x04000   Root directory inode
    ///    5    0x05000   Root dir B-tree node
    ///    6    0x06000   File inode (file.txt)
    ///    7    0x07000   File data block
    ///    8    0x08000   Subdir inode
    ///    9    0x09000   Subdir B-tree node
    ///   10    0x0A000   Nested file inode (nested.dat)
    ///   11    0x0B000   Nested file data block
    ///   12    0x0C000   Deleted file inode (deleted.txt)
    ///   13    0x0D000   Deleted file data block
    fn build_two_checkpoint_fixture() -> Vec<u8> {
        let block_size: usize = 4096;
        let total_blocks: usize = 14;
        let total_size = total_blocks * block_size;
        let mut img = vec![0u8; total_size];

        let block = |n: usize| -> usize { n * block_size };

        // ── Block 0: Container superblock ──
        let csb = &mut img[block(0)..block(1)];
        csb[NX_MAGIC_OFF..NX_MAGIC_OFF + 4].copy_from_slice(&NXSB_MAGIC.to_le_bytes());
        csb[NX_BLOCK_SIZE_OFF..NX_BLOCK_SIZE_OFF + 4].copy_from_slice(&4096u32.to_le_bytes());
        csb[NX_XP_DESC_BLOCKS_OFF..NX_XP_DESC_BLOCKS_OFF + 4].copy_from_slice(&2u32.to_le_bytes());
        csb[NX_XP_DESC_BASE_OFF..NX_XP_DESC_BASE_OFF + 8].copy_from_slice(&1u64.to_le_bytes());
        csb[NX_XP_DESC_INDEX_OFF..NX_XP_DESC_INDEX_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        csb[NX_XP_DESC_LEN_OFF..NX_XP_DESC_LEN_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        csb[NX_XP_DESC_NEXT_OFF..NX_XP_DESC_NEXT_OFF + 4].copy_from_slice(&2u32.to_le_bytes());
        csb[NX_MAX_FILE_SYSTEMS_OFF..NX_MAX_FILE_SYSTEMS_OFF + 4]
            .copy_from_slice(&1u32.to_le_bytes());
        csb[NX_FS_OID_OFF..NX_FS_OID_OFF + 8].copy_from_slice(&100u64.to_le_bytes());

        // ── Helper: write a fixed-KV checkpoint B-tree node ──
        fn write_checkpoint_node(node: &mut [u8], mappings: &[(u64, u64)]) {
            let nkeys = mappings.len() as u32;
            let key_size: u16 = 8;
            let val_size: u16 = 8;
            let entry_data_size = (key_size + val_size) as usize;

            node[BT_FLAGS_OFF..BT_FLAGS_OFF + 2]
                .copy_from_slice(&(BT_ROOT | BT_LEAF | BT_FIXED_KV).to_le_bytes());
            node[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
            node[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&nkeys.to_le_bytes());

            let table_off: u16 = BT_TOC_BASE as u16;
            let table_len: u16 =
                (nkeys as usize * TOC_ENTRY_SIZE + nkeys as usize * entry_data_size) as u16;
            node[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2]
                .copy_from_slice(&table_off.to_le_bytes());
            node[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4]
                .copy_from_slice(&table_len.to_le_bytes());

            let kv_data_start = table_off as usize + nkeys as usize * TOC_ENTRY_SIZE;
            for (i, &(oid, blk)) in mappings.iter().enumerate() {
                let toc_off = table_off as usize + i * TOC_ENTRY_SIZE;
                let key_data_off = kv_data_start + i * entry_data_size;
                let val_data_off = key_data_off + key_size as usize;

                node[toc_off..toc_off + 2].copy_from_slice(&(key_data_off as u16).to_le_bytes());
                node[toc_off + 2..toc_off + 4].copy_from_slice(&key_size.to_le_bytes());
                node[toc_off + 4..toc_off + 6]
                    .copy_from_slice(&(val_data_off as u16).to_le_bytes());
                node[toc_off + 6..toc_off + 8].copy_from_slice(&val_size.to_le_bytes());

                node[key_data_off..key_data_off + 8].copy_from_slice(&oid.to_le_bytes());
                node[val_data_off..val_data_off + 8].copy_from_slice(&blk.to_le_bytes());
            }
        }

        // ── Block 1: Checkpoint [0] — older, includes deleted file (OID 1400→12, 1500→13) ──
        let older_mappings: Vec<(u64, u64)> = vec![
            (100, 3),
            (200, 4),
            (300, 5),
            (400, 6),
            (500, 7),
            (600, 8),
            (700, 9),
            (800, 10),
            (900, 11),
            (1400, 12),
            (1500, 13),
        ];
        write_checkpoint_node(&mut img[block(1)..block(2)], &older_mappings);

        // ── Block 2: Checkpoint [1] — current, without deleted file ──
        let current_mappings: Vec<(u64, u64)> = vec![
            (100, 3),
            (200, 4),
            (300, 5),
            (400, 6),
            (500, 7),
            (600, 8),
            (700, 9),
            (800, 10),
            (900, 11),
        ];
        write_checkpoint_node(&mut img[block(2)..block(3)], &current_mappings);

        // ── Block 3: Volume superblock (APSB) ──
        let vsb = &mut img[block(3)..block(4)];
        vsb[AP_MAGIC_OFF..AP_MAGIC_OFF + 4].copy_from_slice(&APSB_MAGIC.to_le_bytes());
        vsb[AP_ROOT_TREE_OID_OFF..AP_ROOT_TREE_OID_OFF + 8].copy_from_slice(&200u64.to_le_bytes());

        // ── Block 4: Root directory inode ──
        let rdi = &mut img[block(4)..block(5)];
        rdi[0..8].copy_from_slice(&0u64.to_le_bytes()); // parent_id
        rdi[8..16].copy_from_slice(&200u64.to_le_bytes()); // private_id
        rdi[0x10..0x18].copy_from_slice(&700_000_000_000u64.to_le_bytes()); // create
        rdi[0x18..0x20].copy_from_slice(&700_000_000_001u64.to_le_bytes()); // mod
        rdi[0x20..0x28].copy_from_slice(&700_000_000_002u64.to_le_bytes()); // change
        rdi[0x28..0x30].copy_from_slice(&700_000_000_003u64.to_le_bytes()); // access
        rdi[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        rdi[0x38..0x3C].copy_from_slice(&5u32.to_le_bytes());
        rdi[0x44..0x48].copy_from_slice(&501u32.to_le_bytes());
        rdi[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes());
        rdi[0x4C..0x4E].copy_from_slice(&S_IFDIR.to_le_bytes());
        rdi[0x58..0x60].copy_from_slice(&0u64.to_le_bytes());
        rdi[0x80..0x88].copy_from_slice(&300u64.to_le_bytes()); // children_oid

        // ── Block 5: Root dir B-tree node ──
        let rdb = &mut img[block(5)..block(6)];
        rdb[BT_FLAGS_OFF..BT_FLAGS_OFF + 2].copy_from_slice(&(BT_ROOT | BT_LEAF).to_le_bytes());
        rdb[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
        rdb[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&3u32.to_le_bytes());

        let dir_toc_start = BT_TOC_BASE;
        rdb[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2]
            .copy_from_slice(&(dir_toc_start as u16).to_le_bytes());
        rdb[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4].copy_from_slice(&512u16.to_le_bytes());

        let mut kv_start = dir_toc_start + (3usize * TOC_ENTRY_SIZE);
        // Entry 0: file.txt (OID 400)
        write_dir_entry(
            rdb,
            0,
            dir_toc_start,
            &mut kv_start,
            b"file.txt",
            400,
            false,
        );
        // Entry 1: subdir (OID 600)
        write_dir_entry(rdb, 1, dir_toc_start, &mut kv_start, b"subdir", 600, true);
        // Entry 2: deleted.txt (OID 1400) — visible in checkpoint [0], not in [1]
        write_dir_entry(
            rdb,
            2,
            dir_toc_start,
            &mut kv_start,
            b"deleted.txt",
            1400,
            false,
        );

        // ── Block 6: file.txt inode ──
        write_file_inode(
            &mut img,
            block(6),
            200,
            400,
            500,
            b"Hello from APFS!",
            700_000_000_100,
        );

        // ── Block 7: file.txt data ──
        img[block(7)..block(7) + 16].copy_from_slice(b"Hello from APFS!");

        // ── Block 8: subdir inode ──
        let sdi = &mut img[block(8)..block(9)];
        sdi[0..8].copy_from_slice(&200u64.to_le_bytes());
        sdi[8..16].copy_from_slice(&600u64.to_le_bytes());
        sdi[0x10..0x18].copy_from_slice(&700_001_000_000u64.to_le_bytes());
        sdi[0x18..0x20].copy_from_slice(&700_001_000_001u64.to_le_bytes());
        sdi[0x20..0x28].copy_from_slice(&700_001_000_002u64.to_le_bytes());
        sdi[0x28..0x30].copy_from_slice(&700_001_000_003u64.to_le_bytes());
        sdi[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        sdi[0x38..0x3C].copy_from_slice(&2u32.to_le_bytes());
        sdi[0x44..0x48].copy_from_slice(&501u32.to_le_bytes());
        sdi[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes());
        sdi[0x4C..0x4E].copy_from_slice(&S_IFDIR.to_le_bytes());
        sdi[0x58..0x60].copy_from_slice(&0u64.to_le_bytes());
        sdi[0x80..0x88].copy_from_slice(&700u64.to_le_bytes());

        // ── Block 9: subdir B-tree node ──
        let sdb = &mut img[block(9)..block(10)];
        sdb[BT_FLAGS_OFF..BT_FLAGS_OFF + 2].copy_from_slice(&(BT_ROOT | BT_LEAF).to_le_bytes());
        sdb[BT_LEVEL_OFF..BT_LEVEL_OFF + 2].copy_from_slice(&0u16.to_le_bytes());
        sdb[BT_NKEYS_OFF..BT_NKEYS_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        sdb[BT_TABLE_SPACE_OFF..BT_TABLE_SPACE_OFF + 2]
            .copy_from_slice(&(dir_toc_start as u16).to_le_bytes());
        sdb[BT_TABLE_SPACE_OFF + 2..BT_TABLE_SPACE_OFF + 4].copy_from_slice(&512u16.to_le_bytes());

        let mut kv_start2 = dir_toc_start + TOC_ENTRY_SIZE;
        write_dir_entry(
            sdb,
            0,
            dir_toc_start,
            &mut kv_start2,
            b"nested.dat",
            800,
            false,
        );

        // ── Block 10: nested.dat inode ──
        write_file_inode(
            &mut img,
            block(10),
            600,
            800,
            900,
            b"Nested APFS data",
            700_002_000_000,
        );

        // ── Block 11: nested.dat data ──
        img[block(11)..block(11) + 16].copy_from_slice(b"Nested APFS data");

        // ── Block 12: deleted.txt inode ──
        write_file_inode(
            &mut img,
            block(12),
            200,
            1400,
            1500,
            b"Deleted content!",
            700_010_000_000,
        );

        // ── Block 13: deleted.txt data ──
        img[block(13)..block(13) + 16].copy_from_slice(b"Deleted content!");

        img
    }

    /// Write a variable-length directory KV entry into a B-tree node.
    fn write_dir_entry(
        node: &mut [u8],
        toc_idx: usize,
        toc_start: usize,
        kv_start: &mut usize,
        name: &[u8],
        file_id: u64,
        is_dir: bool,
    ) {
        let key_size = 10 + name.len();
        let val_size = 32;
        let toc_off = toc_start + toc_idx * TOC_ENTRY_SIZE;

        node[toc_off..toc_off + 2].copy_from_slice(&(*kv_start as u16).to_le_bytes());
        node[toc_off + 2..toc_off + 4].copy_from_slice(&(key_size as u16).to_le_bytes());
        let val_off = *kv_start + key_size;
        node[toc_off + 4..toc_off + 6].copy_from_slice(&(val_off as u16).to_le_bytes());
        node[toc_off + 6..toc_off + 8].copy_from_slice(&(val_size as u16).to_le_bytes());

        node[*kv_start..*kv_start + 8].copy_from_slice(&file_id.to_le_bytes());
        node[*kv_start + 8..*kv_start + 10].copy_from_slice(&(name.len() as u16).to_le_bytes());
        node[*kv_start + 10..*kv_start + 10 + name.len()].copy_from_slice(name);

        node[val_off..val_off + 8].copy_from_slice(&file_id.to_le_bytes());
        node[val_off + 8..val_off + 16].copy_from_slice(&700_000_000_000u64.to_le_bytes());
        if is_dir {
            node[val_off + 24..val_off + 26].copy_from_slice(&DREC_TYPE_DIR.to_le_bytes());
        } else {
            node[val_off + 24..val_off + 26].copy_from_slice(&DREC_TYPE_FILE.to_le_bytes());
        }

        *kv_start += key_size + val_size;
    }

    /// Write a file inode (parent_id, oid, extent_oid, content, base_timestamp).
    fn write_file_inode(
        img: &mut [u8],
        blk_start: usize,
        parent_id: u64,
        oid: u64,
        extent_oid: u64,
        content: &[u8],
        base_time: u64,
    ) {
        let fi = &mut img[blk_start..blk_start + 4096];
        fi[0..8].copy_from_slice(&parent_id.to_le_bytes());
        fi[8..16].copy_from_slice(&oid.to_le_bytes());
        fi[0x10..0x18].copy_from_slice(&base_time.to_le_bytes());
        fi[0x18..0x20].copy_from_slice(&(base_time + 1).to_le_bytes());
        fi[0x20..0x28].copy_from_slice(&(base_time + 2).to_le_bytes());
        fi[0x28..0x30].copy_from_slice(&(base_time + 3).to_le_bytes());
        fi[0x30..0x38].copy_from_slice(&0u64.to_le_bytes());
        fi[0x38..0x3C].copy_from_slice(&1u32.to_le_bytes());
        fi[0x44..0x48].copy_from_slice(&501u32.to_le_bytes());
        fi[0x48..0x4C].copy_from_slice(&20u32.to_le_bytes());
        fi[0x4C..0x4E].copy_from_slice(&S_IFREG.to_le_bytes());
        fi[0x58..0x60].copy_from_slice(&(content.len() as u64).to_le_bytes());
        fi[0x80..0x88].copy_from_slice(&0u64.to_le_bytes());
        fi[0x88..0x90].copy_from_slice(&extent_oid.to_le_bytes());
    }

    // -------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------

    #[test]
    fn test_checkpoint_descriptor_parsing() {
        let img = build_two_checkpoint_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        let desc = apfs.parse_checkpoint_descriptor().unwrap();
        assert_eq!(desc.base_block, 1);
        assert_eq!(desc.block_count, 2);
        assert_eq!(desc.active_index, 1);
        assert_eq!(desc.data_length, 1);
        assert_eq!(desc.next_index, 2);
    }

    #[test]
    fn test_rewind_to_older_checkpoint() {
        let img = build_two_checkpoint_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        // Rewind to checkpoint 0 (older — has deleted.txt).
        let files = apfs.rewind_to_checkpoint(0).unwrap();

        let paths: Vec<&str> = files.iter().map(|n| n.path.as_str()).collect();
        assert!(
            paths.contains(&"Macintosh HD/file.txt"),
            "older checkpoint should have file.txt, got {:?}",
            paths
        );
        assert!(
            paths.contains(&"Macintosh HD/deleted.txt"),
            "older checkpoint should have deleted.txt, got {:?}",
            paths
        );
        assert!(
            paths.contains(&"Macintosh HD/subdir/nested.dat"),
            "older checkpoint should have nested.dat, got {:?}",
            paths
        );
    }

    #[test]
    fn test_rewind_to_current_checkpoint() {
        let img = build_two_checkpoint_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        // Rewind to checkpoint 1 (current — no deleted.txt).
        let files = apfs.rewind_to_checkpoint(1).unwrap();

        let paths: Vec<&str> = files.iter().map(|n| n.path.as_str()).collect();
        assert!(
            paths.contains(&"Macintosh HD/file.txt"),
            "current checkpoint should have file.txt, got {:?}",
            paths
        );
        assert!(
            !paths.contains(&"Macintosh HD/deleted.txt"),
            "current checkpoint should NOT have deleted.txt, got {:?}",
            paths
        );
    }

    #[test]
    fn test_diff_checkpoints() {
        let img = build_two_checkpoint_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        // Diff cp0 (older) vs cp1 (current).
        let diff = apfs.diff_checkpoints(0, 1).unwrap();

        // deleted.txt should be in removed (present in cp0, absent in cp1).
        let removed_paths: Vec<&str> = diff.removed.iter().map(|n| n.path.as_str()).collect();
        assert!(
            removed_paths.contains(&"Macintosh HD/deleted.txt"),
            "diff should show deleted.txt as removed, got {:?}",
            removed_paths
        );

        // No files should be added (cp1 is a subset of cp0).
        assert!(
            diff.added.is_empty(),
            "no files should be added, got {:?}",
            diff.added
        );

        // No files should change (sizes and timestamps are the same).
        assert!(
            diff.changed.is_empty(),
            "no files should be changed, got {:?}",
            diff.changed
        );

        // Reverse diff: cp1 → cp0 should show deleted.txt as added.
        let rev = apfs.diff_checkpoints(1, 0).unwrap();
        let rev_added: Vec<&str> = rev.added.iter().map(|n| n.path.as_str()).collect();
        assert!(
            rev_added.contains(&"Macintosh HD/deleted.txt"),
            "reverse diff should show deleted.txt as added, got {:?}",
            rev_added
        );
        assert!(
            rev.removed.is_empty(),
            "reverse diff should have no removals"
        );
    }

    #[test]
    fn test_recover_deleted_files() {
        let img = build_two_checkpoint_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        let recovered = apfs.recover_deleted_files().unwrap();

        // deleted.txt (from checkpoint 0) should be recovered.
        let paths: Vec<&str> = recovered.iter().map(|r| r.path.as_str()).collect();
        assert!(
            paths.contains(&"Macintosh HD/deleted.txt"),
            "recovery should find deleted.txt, got {:?}",
            paths
        );

        // It should come from checkpoint index 0.
        let deleted = recovered
            .iter()
            .find(|r| r.path == "Macintosh HD/deleted.txt")
            .unwrap();
        assert_eq!(deleted.checkpoint_index, 0);
        assert_eq!(deleted.node.size, 16);
        assert!(!deleted.node.is_dir);
    }

    #[test]
    fn test_diff_file_lists_empty() {
        let diff = diff_file_lists(&[], &[]);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn test_diff_file_lists_changed_size() {
        let f1 = vec![fs_node("a.txt", false, 10, None, None, None)];
        let mut f2_node = fs_node("a.txt", false, 20, None, None, None);
        f2_node.path = "a.txt".to_string();
        let f1_with_path: Vec<FsNode> = f1
            .into_iter()
            .map(|mut n| {
                n.path = n.name.clone();
                n
            })
            .collect();

        let diff = diff_file_lists(&f1_with_path, &[f2_node]);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].old_size, 10);
        assert_eq!(diff.changed[0].new_size, 20);
    }

    #[test]
    fn test_checkpoint_out_of_range() {
        let img = build_two_checkpoint_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let apfs = ApfsReader::open(reader, 0).unwrap();

        // Checkpoint index 99 exceeds the area and will read garbage / fail.
        let result = apfs.rewind_to_checkpoint(99);
        // This should eventually fail because the block read won't have valid
        // OID mappings for essential OIDs (volume superblock, etc.).
        assert!(result.is_err());
    }
}
