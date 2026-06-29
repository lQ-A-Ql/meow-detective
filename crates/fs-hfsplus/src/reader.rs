//! HFS+ filesystem reader implementation.

use crate::constants::*;
use crate::parser::{
    parse_catalog_record, read_u16_be, read_u32_be, BtNodeDesc, CatalogRecord, HfsForkData,
};
use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node, invalid_fs_data, path_components,
    path_is_directory, path_not_found, root_node, FileSystemReader, FsNode,
};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

pub struct HfsPlusReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    volume_offset: u64,
    block_size: u32,
    total_blocks: u32,
    free_blocks: u32,
    /// The catalog file fork data.
    catalog_fork: HfsForkData,
    /// Catalog B-tree header node data (parsed for format completeness).
    _catalog_header_node: Vec<u8>,
    /// Root node number for the catalog B-tree.
    catalog_root_node: u32,
    /// Node size for the catalog B-tree.
    catalog_node_size: u16,
}

impl HfsPlusReader {
    /// Open an HFS+ volume at the given logical offset of the provided reader.
    ///
    /// The `reader` should be positioned so that `/dev/diskXsY` (or equivalent)
    /// begins at offset 0.  `offset` is a byte offset into the raw evidence for
    /// cases where the HFS+ partition does not start at the beginning of the
    /// evidence image.
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        // Read the volume header (512 bytes at offset + 1024).
        let vh_absolute = offset + VOLUME_HEADER_OFFSET;
        let mut vh_buf = [0u8; VOLUME_HEADER_SIZE];
        reader.seek(SeekFrom::Start(vh_absolute))?;
        reader.read_exact(&mut vh_buf)?;

        // Validate magic.
        let signature = read_u16_be(&vh_buf, VH_SIGNATURE);
        if signature != HFSPLUS_SIGNATURE && signature != HFSX_SIGNATURE {
            return Err(invalid_fs_data(format!(
                "not a valid HFS+ volume (magic 0x{:04X})",
                signature
            )));
        }

        let block_size = read_u32_be(&vh_buf, VH_BLOCK_SIZE);
        if block_size == 0 || block_size < 512 {
            return Err(invalid_fs_data("invalid HFS+ block size"));
        }

        let total_blocks = read_u32_be(&vh_buf, VH_TOTAL_BLOCKS);
        let free_blocks = read_u32_be(&vh_buf, VH_FREE_BLOCKS);

        // Parse the catalog file fork data from the volume header.
        let catalog_fork = HfsForkData::from_bytes(&vh_buf, VH_CATALOG_FILE);
        if catalog_fork.extents.is_empty() {
            return Err(invalid_fs_data("HFS+ catalog file has no extents"));
        }

        // Read the first node of the catalog B-tree (the header node).
        let first_extent = &catalog_fork.extents[0];
        let first_node_off = offset + first_extent.start_block as u64 * block_size as u64;

        // We need the node size from the header node. Temporarily read a generous
        // buffer to find the node size field.
        let mut node_buf = vec![0u8; block_size as usize];
        reader.seek(SeekFrom::Start(first_node_off))?;
        reader.read_exact(&mut node_buf)?;

        let node_desc = BtNodeDesc::parse(&node_buf)?;
        if node_desc.kind != BT_HEADER_NODE {
            return Err(invalid_fs_data(
                "catalog B-tree first node is not a header node",
            ));
        }

        // The header record is the first record in the header node.
        let offsets = node_desc.record_offsets(&node_buf);
        if offsets.is_empty() {
            return Err(invalid_fs_data("catalog B-tree header node has no records"));
        }
        let header_record_data = node_desc.get_record_data(&node_buf, offsets[0]);

        // Parse the B-tree header record.
        // The header record has a key (keyLength u16 + parentCNID u32 + nodeName with 0 chars)
        // followed by the header data. The key for the header node is fixed: 0x0000 (keyLength),
        // 0x00000000 (parentCNID), 0x0000 (zero-length name) = 8 bytes.
        let header_data = if header_record_data.len() > 8 {
            // Skip the key portion (keyLength=0x0000 + parentCNID=0 + nameLen=0 = 8 bytes).
            &header_record_data[8..]
        } else {
            return Err(invalid_fs_data("catalog B-tree header record too short"));
        };

        if header_data.len() < 32 {
            return Err(invalid_fs_data("catalog B-tree header data too short"));
        }

        let catalog_root_node = read_u32_be(header_data, BT_HEADER_ROOT_NODE);
        let catalog_node_size = read_u16_be(header_data, BT_HEADER_NODE_SIZE);
        let _total_nodes = read_u32_be(header_data, BT_HEADER_TOTAL_NODES);

        Ok(Self {
            reader: RefCell::new(reader),
            volume_offset: offset,
            block_size,
            total_blocks,
            free_blocks,
            catalog_fork,
            _catalog_header_node: node_buf,
            catalog_root_node,
            catalog_node_size,
        })
    }

    /// Public accessors for the parsed volume header fields.
    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    pub fn total_blocks(&self) -> u32 {
        self.total_blocks
    }

    pub fn free_blocks(&self) -> u32 {
        self.free_blocks
    }

    // -----------------------------------------------------------------------
    // B-tree helpers
    // -----------------------------------------------------------------------

    /// Read a B-tree node by node number.
    fn read_btree_node(&self, node_number: u32) -> io::Result<Vec<u8>> {
        // Catalog B-tree nodes are stored in the catalog fork's extents.
        let mut remaining = node_number as u64 * self.catalog_node_size as u64;
        for extent in &self.catalog_fork.extents {
            let extent_bytes = extent.block_count as u64 * self.block_size as u64;
            if remaining < extent_bytes {
                let offset = self.volume_offset
                    + extent.start_block as u64 * self.block_size as u64
                    + remaining;
                let mut buf = vec![0u8; self.catalog_node_size as usize];
                let mut reader = self.reader.borrow_mut();
                reader.seek(SeekFrom::Start(offset))?;
                reader.read_exact(&mut buf)?;
                return Ok(buf);
            }
            remaining = remaining.saturating_sub(extent_bytes);
        }
        Err(invalid_fs_data(format!(
            "catalog B-tree node {} out of extent range",
            node_number
        )))
    }

    /// Recursively descend the B-tree to find all records with a given parent CNID.
    fn find_records_for_parent(
        &self,
        node_number: u32,
        parent_cnid: u32,
    ) -> io::Result<Vec<CatalogRecord>> {
        let node_data = self.read_btree_node(node_number)?;
        let desc = BtNodeDesc::parse(&node_data)?;

        match desc.kind {
            BT_LEAF_NODE => {
                // Gather all matching records from this leaf.
                let offsets = desc.record_offsets(&node_data);
                let mut records = Vec::new();

                // Determine the size of the key portion: keyLength(u16) bytes.
                // For leaf nodes, each record is key + catalog data.
                // We only know the split after parsing the key.
                for &rec_off in &offsets {
                    let rec_bytes = desc.get_record_data(&node_data, rec_off);
                    if rec_bytes.len() < 8 {
                        continue;
                    }
                    let key_length = read_u16_be(rec_bytes, 0) as usize;
                    if key_length < 2 || key_length > rec_bytes.len() {
                        continue;
                    }
                    let key_data = &rec_bytes[0..key_length];
                    let val_data = &rec_bytes[key_length..];

                    // Check parentCNID in the key (at key_data offset 2).
                    if key_data.len() < 6 {
                        continue;
                    }
                    let rec_parent = read_u32_be(key_data, 2);
                    if rec_parent != parent_cnid {
                        continue;
                    }

                    match parse_catalog_record(key_data, val_data) {
                        Ok(Some(record)) => records.push(record),
                        Ok(None) => {} // skip non-folder/non-file records
                        Err(_) => {}   // skip unparseable records
                    }
                }
                Ok(records)
            }
            BT_INDEX_NODE => {
                // For index nodes, keys are (parentCNID, nodeName).
                // Each key has an associated child node number (u32) as its "value".
                // We need to find the child node(s) whose key range covers parent_cnid.
                let offsets = desc.record_offsets(&node_data);
                let mut results = Vec::new();

                for &rec_off in &offsets {
                    let rec_bytes = desc.get_record_data(&node_data, rec_off);
                    if rec_bytes.len() < 8 {
                        continue;
                    }
                    let key_length = read_u16_be(rec_bytes, 0) as usize;
                    if key_length < 8 || key_length + 4 > rec_bytes.len() {
                        continue;
                    }
                    let key_data = &rec_bytes[0..key_length];
                    let val_start = key_length;
                    if val_start + 4 > rec_bytes.len() {
                        continue;
                    }
                    let child_node = read_u32_be(rec_bytes, val_start);

                    let key_parent = read_u32_be(key_data, 2);

                    // Index node key comparison: we need to determine whether
                    // parent_cnid falls in the range covered by this key.
                    // For an index node with n records, record i (0-indexed) covers
                    // keys >= key[i] and < key[i+1] (or >= key[i] for the last one).
                    // We check: if parent_cnid > key_parent, skip this key.
                    // If parent_cnid >= key_parent AND (i is last OR parent_cnid < key[i+1]),
                    // then we descend.
                    if parent_cnid < key_parent {
                        // This key and all subsequent keys have larger parentCNID,
                        // so we're done.
                        break;
                    }
                    if parent_cnid == key_parent {
                        // Exact match on parentCNID — descend into this child.
                        if let Ok(recs) = self.find_records_for_parent(child_node, parent_cnid) {
                            results.extend(recs)
                        }
                    } else {
                        // parent_cnid > key_parent: this child might still contain
                        // records for our parent_cnid if this is the last entry.
                        // For simplicity, always descend into the last child that
                        // has key_parent <= parent_cnid.
                        // We'll continue to the next key and only descend into
                        // the rightmost matching child.
                    }
                }

                // If we didn't find an exact match, try the last child that
                // covers the range.
                if results.is_empty() && !offsets.is_empty() {
                    // Walk backward to find the last child with key_parent <= parent_cnid.
                    for &rec_off in offsets.iter().rev() {
                        let rec_bytes = desc.get_record_data(&node_data, rec_off);
                        if rec_bytes.len() < 12 {
                            continue;
                        }
                        let key_length = read_u16_be(rec_bytes, 0) as usize;
                        if key_length < 8 || key_length + 4 > rec_bytes.len() {
                            continue;
                        }
                        let key_data = &rec_bytes[0..key_length];
                        let key_parent = read_u32_be(key_data, 2);
                        let child_node = read_u32_be(rec_bytes, key_length);
                        if key_parent <= parent_cnid {
                            match self.find_records_for_parent(child_node, parent_cnid) {
                                Ok(recs) => {
                                    results.extend(recs);
                                    break;
                                }
                                Err(_) => continue,
                            }
                        }
                    }
                }

                Ok(results)
            }
            BT_HEADER_NODE => {
                // Should not encounter a header node during traversal.
                Err(invalid_fs_data("unexpected header node during B-tree walk"))
            }
            _ => Err(invalid_fs_data(format!(
                "unknown B-tree node kind 0x{:02X}",
                desc.kind
            ))),
        }
    }

    /// Read file content from a data fork via extent descriptors.
    fn read_data_fork(&self, fork: &HfsForkData) -> io::Result<Vec<u8>> {
        let mut reader = self.reader.borrow_mut();
        fork.read_all(&mut *reader, self.volume_offset, self.block_size)
    }

    /// Resolve a path relative to a starting CNID (e.g., root = 2).
    /// Returns (cnid, is_dir) or None if not found.
    fn resolve_path_from_cnid(
        &self,
        start_cnid: u32,
        path: &str,
    ) -> io::Result<Option<(u32, bool)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((start_cnid, true)));
        }

        let mut current_cnid = start_cnid;
        for (i, comp) in components.iter().enumerate() {
            let records = self.find_records_for_parent(self.catalog_root_node, current_cnid)?;
            let is_last = i == components.len() - 1;

            // Case-insensitive match first (common in HFS+), then case-sensitive.
            let found = records.iter().find(|r| r.name.eq_ignore_ascii_case(comp));
            let found = found.or_else(|| records.iter().find(|r| r.name == *comp));

            match found {
                Some(record) => {
                    if is_last {
                        return Ok(Some((record.cnid, record.is_dir)));
                    }
                    if !record.is_dir {
                        return Ok(None);
                    }
                    current_cnid = record.cnid;
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// FileSystemReader implementation
// ---------------------------------------------------------------------------

impl FileSystemReader for HfsPlusReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        // Root path ("", "/", "\\") → list contents of root directory (CNID=2).
        let normalized = path.trim_matches(['/', '\\']);
        let cnid = if normalized.is_empty() {
            // kHFSRootFolderID = 2
            2u32
        } else {
            // Path resolution from root.
            let resolved = self
                .resolve_path_from_cnid(2, path)?
                .ok_or_else(|| path_not_found(path))?;
            if !resolved.1 {
                // CNID 1 is the root folder's parent (kHFSRootParentID).
                return Err(evidence_core::filesystem::path_is_not_directory(path));
            }
            resolved.0
        };

        let records = self.find_records_for_parent(self.catalog_root_node, cnid)?;
        let parent_for_path = if normalized.is_empty() { "" } else { path };

        let nodes: Vec<FsNode> = records
            .into_iter()
            .map(|r| {
                fs_node(
                    r.name.clone(),
                    r.is_dir,
                    if r.is_dir { 0 } else { r.logical_size },
                    crate::parser::hfs_timestamp_to_dt(r.create_date),
                    crate::parser::hfs_timestamp_to_dt(r.content_mod_date),
                    crate::parser::hfs_timestamp_to_dt(r.access_date),
                )
            })
            .collect();

        Ok(child_nodes_with_parent_path(nodes, parent_for_path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        // Resolve from root CNID=2.
        let resolved = self
            .resolve_path_from_cnid(2, path)?
            .ok_or_else(|| file_not_found(path))?;

        if resolved.1 {
            return Err(path_is_directory(path));
        }

        // We need the file's catalog record with its data fork.
        // Find the record by searching with parent CNID and file name.
        let parent_path = {
            let last_slash = path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
            if last_slash == 0 {
                "\\"
            } else {
                &path[..last_slash.saturating_sub(1)]
            }
        };
        let file_name = path
            .rfind(['/', '\\'])
            .map(|i| &path[i + 1..])
            .unwrap_or(path);

        let parent_cnid = if parent_path.is_empty() || parent_path == "/" || parent_path == "\\" {
            2u32
        } else {
            self.resolve_path_from_cnid(2, parent_path)?
                .map(|(cnid, _)| cnid)
                .ok_or_else(|| file_not_found(path))?
        };

        let records = self.find_records_for_parent(self.catalog_root_node, parent_cnid)?;
        let file_record = records
            .iter()
            .find(|r| !r.is_dir && r.name.eq_ignore_ascii_case(file_name))
            .or_else(|| records.iter().find(|r| !r.is_dir && r.name == file_name))
            .ok_or_else(|| file_not_found(path))?;

        let fork = file_record
            .data_fork
            .as_ref()
            .ok_or_else(|| invalid_fs_data("file record has no data fork"))?;
        let data = self.read_data_fork(fork)?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn data_source_name(&self) -> &str {
        "hfsplus"
    }
}
