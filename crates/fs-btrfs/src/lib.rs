//! Btrfs filesystem reader.
//!
//! Implements the `FileSystemReader` trait for Btrfs-formatted volumes.
//! Parses the superblock at offset 0x10000 (magic `_BHRfS_M`), the chunk
//! tree and root tree B-trees, subvolume listings, directory items, and
//! file extent data.
//!
//! Supported features:
//! - Superblock at standard primary offset 0x10000
//! - Chunk tree logical-to-physical address translation
//! - Root tree subvolume enumeration (FS_TREE, etc.)
//! - Directory listing via DIR_INDEX / DIR_ITEM
//! - File content via EXTENT_DATA (inline and regular extents)

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

const BTRFS_SUPERBLOCK_OFFSET: u64 = 0x10000;
const BTRFS_MAGIC: &[u8; 8] = b"_BHRfS_M";

/// B-tree node header size in bytes (items start at this offset).
const BTRFS_HEADER_SIZE: usize = 101;

/// Size of a single item descriptor in a leaf node.
const LEAF_ITEM_SIZE: usize = 25;

/// Size of a single key-pointer in an internal node.
const INTERNAL_ITEM_SIZE: usize = 33;

/// B-tree key size in bytes.
const KEY_SIZE: usize = 17;

// Key types.
const INODE_ITEM_KEY: u8 = 1;
const DIR_ITEM_KEY: u8 = 84;
const DIR_INDEX_KEY: u8 = 96;
const EXTENT_DATA_KEY: u8 = 108;
const ROOT_ITEM_KEY: u8 = 132;
const ROOT_BACKREF_KEY: u8 = 144;
const CHUNK_ITEM_KEY: u8 = 228;

// Well-known object ids.
#[allow(dead_code)]
const CHUNK_TREE_OBJECTID: u64 = 3;
const FS_TREE_OBJECTID: u64 = 5;
const FIRST_FREE_OBJECTID: u64 = 256;

// Directory entry file types.
#[allow(dead_code)]
const FT_REG_FILE: u8 = 1;
const FT_DIR: u8 = 2;
#[allow(dead_code)]
const FT_SYMLINK: u8 = 7;

// Extent types.
const EXTENT_INLINE: u8 = 0;
#[allow(dead_code)]
const EXTENT_REGULAR: u8 = 1;

// Inode mode bits.
#[allow(dead_code)]
const S_IFDIR: u32 = 0o040000;
#[allow(dead_code)]
const S_IFREG: u32 = 0o100000;

// ---------------------------------------------------------------------------
// On-disk structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct BtrfsKey {
    objectid: u64,
    ty: u8,
    offset: u64,
}

impl BtrfsKey {
    fn parse(data: &[u8]) -> Self {
        Self {
            objectid: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            ty: data[8],
            offset: u64::from_le_bytes(data[9..17].try_into().unwrap()),
        }
    }

    #[allow(dead_code)]
    fn to_bytes(&self) -> [u8; KEY_SIZE] {
        let mut buf = [0u8; KEY_SIZE];
        buf[0..8].copy_from_slice(&self.objectid.to_le_bytes());
        buf[8] = self.ty;
        buf[9..17].copy_from_slice(&self.offset.to_le_bytes());
        buf
    }
}

impl PartialOrd for BtrfsKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BtrfsKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.objectid
            .cmp(&other.objectid)
            .then(self.ty.cmp(&other.ty))
            .then(self.offset.cmp(&other.offset))
    }
}

impl PartialEq for BtrfsKey {
    fn eq(&self, other: &Self) -> bool {
        self.objectid == other.objectid && self.ty == other.ty && self.offset == other.offset
    }
}

impl Eq for BtrfsKey {}

#[derive(Debug)]
struct BtrfsHeader {
    #[allow(dead_code)]
    bytenr: u64,
    nritems: u32,
    level: u8,
}

#[derive(Debug)]
struct LeafItem {
    key: BtrfsKey,
    data_offset: u32,
    data_size: u32,
}

#[derive(Debug)]
struct InternalItem {
    key: BtrfsKey,
    blockptr: u64,
}

#[derive(Debug)]
struct BtrfsChunk {
    logical: u64,
    length: u64,
    physical: u64,
}

#[derive(Debug, Clone)]
struct BtrfsSubvol {
    id: u64,
    name: String,
    root_dirid: u64,
    tree_root_bytenr: u64,
}

// ---------------------------------------------------------------------------
// BtrfsReader
// ---------------------------------------------------------------------------

pub struct BtrfsReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    #[allow(dead_code)]
    sectorsize: u32,
    nodesize: u32,
    root_tree_logical: u64,
    chunk_tree_logical: u64,
    volume_offset: u64,
    /// Logical offset -> (physical offset, length)
    chunk_map: Vec<BtrfsChunk>,
    /// All subvolumes discovered from the root tree.
    subvolumes: Vec<BtrfsSubvol>,
    /// Default subvolume root node logical address.
    default_subvol_root_bytenr: u64,
    /// Default subvolume root directory object id.
    default_subvol_root_dirid: u64,
}

impl BtrfsReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset + BTRFS_SUPERBLOCK_OFFSET))?;
        let mut sb = [0u8; 4096];
        reader.read_exact(&mut sb)?;

        let magic = &sb[0x40..0x48];
        if magic != BTRFS_MAGIC {
            return Err(invalid_fs_data(format!(
                "not a valid btrfs filesystem (magic {:02X?})",
                &sb[0x40..0x48]
            )));
        }

        let sectorsize = u32::from_le_bytes(sb[0xB8..0xBC].try_into().unwrap());
        let nodesize = u32::from_le_bytes(sb[0xBC..0xC0].try_into().unwrap());
        let root_tree_logical = u64::from_le_bytes(sb[0x78..0x80].try_into().unwrap());
        let chunk_tree_logical = u64::from_le_bytes(sb[0x80..0x88].try_into().unwrap());

        if sectorsize == 0 || nodesize == 0 {
            return Err(invalid_fs_data("invalid btrfs geometry"));
        }

        // Parse sys_chunk_array for initial chunk mappings.
        let sys_chunk_array_size = u32::from_le_bytes(sb[0xC8..0xCC].try_into().unwrap()) as usize;
        let sys_chunk_start = 0x32B;
        let sys_chunk_end = (sys_chunk_start + sys_chunk_array_size).min(sb.len());
        let chunk_data = &sb[sys_chunk_start..sys_chunk_end];

        let mut reader_obj = Self {
            reader: RefCell::new(reader),
            sectorsize,
            nodesize,
            root_tree_logical,
            chunk_tree_logical,
            volume_offset: offset,
            chunk_map: Vec::new(),
            subvolumes: Vec::new(),
            default_subvol_root_bytenr: 0,
            default_subvol_root_dirid: FIRST_FREE_OBJECTID,
        };

        reader_obj.parse_chunks(chunk_data)?;

        // Read full chunk tree if separate from root tree.
        if reader_obj.chunk_tree_logical != 0
            && reader_obj.chunk_tree_logical != reader_obj.root_tree_logical
        {
            let _ = reader_obj.read_chunk_tree();
        }

        // Discover subvolumes.
        reader_obj.discover_subvolumes()?;

        // Resolve default subvolume.
        if reader_obj.default_subvol_root_bytenr == 0 {
            if let Some(default_sv) = reader_obj
                .subvolumes
                .iter()
                .find(|s| s.name == "default" || s.id == FS_TREE_OBJECTID)
            {
                reader_obj.default_subvol_root_bytenr = default_sv.tree_root_bytenr;
                reader_obj.default_subvol_root_dirid = default_sv.root_dirid;
            } else if let Some(first) = reader_obj.subvolumes.first() {
                reader_obj.default_subvol_root_bytenr = first.tree_root_bytenr;
                reader_obj.default_subvol_root_dirid = first.root_dirid;
            }
        }

        Ok(reader_obj)
    }

    // -------------------------------------------------------------------
    // Chunk mapping
    // -------------------------------------------------------------------

    fn translate_logical(&self, logical: u64) -> io::Result<u64> {
        for chunk in &self.chunk_map {
            if logical >= chunk.logical && logical < chunk.logical + chunk.length {
                return Ok((logical - chunk.logical) + chunk.physical);
            }
        }
        // Fallback: assume identity mapping.
        Ok(logical)
    }

    fn parse_chunks(&mut self, data: &[u8]) -> io::Result<()> {
        let mut pos = 0usize;
        while pos + KEY_SIZE + 8 <= data.len() {
            let key = BtrfsKey::parse(&data[pos..pos + KEY_SIZE]);
            pos += KEY_SIZE;
            if key.ty != CHUNK_ITEM_KEY {
                break;
            }
            if pos + 0x30 > data.len() {
                break;
            }
            let length = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
            let num_stripes = u16::from_le_bytes(data[pos + 0x2C..pos + 0x2E].try_into().unwrap());
            let stripe_offset = pos + 0x30;
            if num_stripes > 0 {
                let phys = u64::from_le_bytes(
                    data[stripe_offset + 8..stripe_offset + 16]
                        .try_into()
                        .unwrap(),
                );
                self.chunk_map.push(BtrfsChunk {
                    logical: key.offset,
                    length,
                    physical: phys,
                });
            }
            let stripe_size: usize = 0x20; // devid(8) + offset(8) + uuid(16)
            pos += 0x30 + num_stripes as usize * stripe_size;
        }
        Ok(())
    }

    fn read_chunk_tree(&mut self) -> io::Result<()> {
        let node_data = self.read_logical_block(self.chunk_tree_logical)?;
        let header = Self::parse_header(&node_data)?;
        if header.level == 0 {
            let items = Self::parse_leaf_items(&node_data, header.nritems);
            for item in &items {
                if item.key.ty == CHUNK_ITEM_KEY {
                    let chunk_data = Self::get_item_data(&node_data, item);
                    self.parse_chunks(chunk_data)?;
                }
            }
        } else {
            let internal = Self::parse_internal_items(&node_data, header.nritems);
            for ii in &internal {
                let child = self.read_logical_block(ii.blockptr)?;
                let ch = Self::parse_header(&child)?;
                if ch.level == 0 {
                    let items = Self::parse_leaf_items(&child, ch.nritems);
                    for item in &items {
                        if item.key.ty == CHUNK_ITEM_KEY {
                            let chunk_data = Self::get_item_data(&child, item);
                            self.parse_chunks(chunk_data)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Subvolume discovery
    // -------------------------------------------------------------------

    fn discover_subvolumes(&mut self) -> io::Result<()> {
        let root_data = self.read_logical_block(self.root_tree_logical)?;
        let header = Self::parse_header(&root_data)?;

        let mut root_items: HashMap<u64, (u64, u64)> = HashMap::new();
        let mut root_names: HashMap<u64, String> = HashMap::new();

        if header.level == 0 {
            Self::scan_root_leaf(&root_data, header.nritems, &mut root_items, &mut root_names);
        } else {
            let internal = Self::parse_internal_items(&root_data, header.nritems);
            for ii in &internal {
                let child = self.read_logical_block(ii.blockptr)?;
                let ch = Self::parse_header(&child)?;
                if ch.level == 0 {
                    Self::scan_root_leaf(&child, ch.nritems, &mut root_items, &mut root_names);
                } else {
                    let si = Self::parse_internal_items(&child, ch.nritems);
                    for s in &si {
                        let leaf = self.read_logical_block(s.blockptr)?;
                        let lh = Self::parse_header(&leaf)?;
                        if lh.level == 0 {
                            Self::scan_root_leaf(
                                &leaf,
                                lh.nritems,
                                &mut root_items,
                                &mut root_names,
                            );
                        }
                    }
                }
            }
        }

        for (id, (bytenr, root_dirid)) in &root_items {
            let name = root_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("subvol_{}", id));
            self.subvolumes.push(BtrfsSubvol {
                id: *id,
                name,
                root_dirid: *root_dirid,
                tree_root_bytenr: *bytenr,
            });
        }
        Ok(())
    }

    fn scan_root_leaf(
        data: &[u8],
        nritems: u32,
        root_items: &mut HashMap<u64, (u64, u64)>,
        root_names: &mut HashMap<u64, String>,
    ) {
        let items = Self::parse_leaf_items(data, nritems);
        for item in &items {
            if item.key.ty == ROOT_ITEM_KEY {
                let rd = Self::get_item_data(data, item);
                if rd.len() >= 184 {
                    let bytenr = u64::from_le_bytes(rd[176..184].try_into().unwrap());
                    let root_dirid = u64::from_le_bytes(rd[168..176].try_into().unwrap());
                    root_items.insert(item.key.objectid, (bytenr, root_dirid));
                }
            } else if item.key.ty == ROOT_BACKREF_KEY {
                let rb = Self::get_item_data(data, item);
                if rb.len() >= 18 {
                    let name_len = u16::from_le_bytes(rb[16..18].try_into().unwrap()) as usize;
                    if rb.len() >= 18 + name_len {
                        let name = String::from_utf8_lossy(&rb[18..18 + name_len]).to_string();
                        root_names.insert(item.key.objectid, name);
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // B-tree operations
    // -------------------------------------------------------------------

    fn parse_header(data: &[u8]) -> io::Result<BtrfsHeader> {
        if data.len() < BTRFS_HEADER_SIZE {
            return Err(invalid_fs_data("btrfs node too short for header"));
        }
        Ok(BtrfsHeader {
            bytenr: u64::from_le_bytes(data[0x30..0x38].try_into().unwrap()),
            nritems: u32::from_le_bytes(data[0x5D..0x61].try_into().unwrap()),
            level: data[0x61],
        })
    }

    fn parse_leaf_items(data: &[u8], nritems: u32) -> Vec<LeafItem> {
        let mut items = Vec::new();
        let base = BTRFS_HEADER_SIZE;
        for i in 0..nritems {
            let off = base + (i as usize) * LEAF_ITEM_SIZE;
            if off + LEAF_ITEM_SIZE > data.len() {
                break;
            }
            let key = BtrfsKey::parse(&data[off..off + KEY_SIZE]);
            let data_offset = u32::from_le_bytes(data[off + 17..off + 21].try_into().unwrap());
            let data_size = u32::from_le_bytes(data[off + 21..off + 25].try_into().unwrap());
            items.push(LeafItem {
                key,
                data_offset,
                data_size,
            });
        }
        items
    }

    fn parse_internal_items(data: &[u8], nritems: u32) -> Vec<InternalItem> {
        let mut items = Vec::new();
        let base = BTRFS_HEADER_SIZE;
        for i in 0..nritems {
            let off = base + (i as usize) * INTERNAL_ITEM_SIZE;
            if off + INTERNAL_ITEM_SIZE > data.len() {
                break;
            }
            let key = BtrfsKey::parse(&data[off..off + KEY_SIZE]);
            let blockptr = u64::from_le_bytes(data[off + 17..off + 25].try_into().unwrap());
            items.push(InternalItem { key, blockptr });
        }
        items
    }

    fn get_item_data<'a>(node_data: &'a [u8], item: &LeafItem) -> &'a [u8] {
        let start = item.data_offset as usize;
        let end = (start + item.data_size as usize).min(node_data.len());
        &node_data[start..end]
    }

    fn find_items_by_object_and_type(
        items: &[LeafItem],
        objectid: u64,
        key_type: u8,
    ) -> Vec<usize> {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.key.objectid == objectid && item.key.ty == key_type)
            .map(|(i, _)| i)
            .collect()
    }

    /// Walk the B-tree from root_bytenr down to the leaf containing search_key.
    fn walk_to_leaf(
        &self,
        root_bytenr: u64,
        search_key: &BtrfsKey,
    ) -> io::Result<(Vec<u8>, Vec<LeafItem>)> {
        let node_data = self.read_logical_block(root_bytenr)?;
        let header = Self::parse_header(&node_data)?;
        if header.level == 0 {
            let items = Self::parse_leaf_items(&node_data, header.nritems);
            return Ok((node_data, items));
        }
        let internal = Self::parse_internal_items(&node_data, header.nritems);
        let idx = internal
            .binary_search_by(|ii| ii.key.cmp(search_key))
            .unwrap_or_else(|i| i.saturating_sub(1));
        let child_idx = idx.min(internal.len().saturating_sub(1));
        if let Some(ii) = internal.get(child_idx) {
            self.walk_to_leaf(ii.blockptr, search_key)
        } else {
            Err(invalid_fs_data("empty btrfs internal node"))
        }
    }

    fn read_logical_block(&self, logical: u64) -> io::Result<Vec<u8>> {
        let physical = self.translate_logical(logical)?;
        let absolute = self.volume_offset + physical;
        let mut buf = vec![0u8; self.nodesize as usize];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(absolute))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    // -------------------------------------------------------------------
    // Directory & file operations
    // -------------------------------------------------------------------

    fn parse_dir_entry(data: &[u8]) -> Option<(String, u64, u8)> {
        if data.len() < 30 {
            return None;
        }
        let child_obj = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let name_len = u16::from_le_bytes(data[27..29].try_into().unwrap()) as usize;
        let file_type = data[29];
        if data.len() < 30 + name_len {
            return None;
        }
        let name = String::from_utf8_lossy(&data[30..30 + name_len]).to_string();
        if name.is_empty() {
            return None;
        }
        Some((name, child_obj, file_type))
    }

    fn list_dir_entries(
        &self,
        tree_root_bytenr: u64,
        dir_objectid: u64,
    ) -> io::Result<Vec<(String, u64, u8)>> {
        let search_key = BtrfsKey {
            objectid: dir_objectid,
            ty: DIR_INDEX_KEY,
            offset: 0,
        };
        let (leaf_data, items) = self.walk_to_leaf(tree_root_bytenr, &search_key)?;
        let indices = Self::find_items_by_object_and_type(&items, dir_objectid, DIR_INDEX_KEY);

        let mut entries = Vec::new();
        for &idx in &indices {
            let item_data = Self::get_item_data(&leaf_data, &items[idx]);
            if let Some(entry) = Self::parse_dir_entry(item_data) {
                entries.push(entry);
            }
        }

        // Fallback to DIR_ITEM if no DIR_INDEX entries.
        if entries.is_empty() {
            let ditem = Self::find_items_by_object_and_type(&items, dir_objectid, DIR_ITEM_KEY);
            for &idx in &ditem {
                let item_data = Self::get_item_data(&leaf_data, &items[idx]);
                if let Some(entry) = Self::parse_dir_entry(item_data) {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }

    fn read_file_extents(
        &self,
        tree_root_bytenr: u64,
        inode_objectid: u64,
        declared_size: u64,
    ) -> io::Result<Vec<u8>> {
        let search_key = BtrfsKey {
            objectid: inode_objectid,
            ty: EXTENT_DATA_KEY,
            offset: 0,
        };
        let (leaf_data, items) = self.walk_to_leaf(tree_root_bytenr, &search_key)?;
        let indices = Self::find_items_by_object_and_type(&items, inode_objectid, EXTENT_DATA_KEY);

        let mut data = Vec::new();
        for &idx in &indices {
            let item_data = Self::get_item_data(&leaf_data, &items[idx]);
            if item_data.len() < 21 {
                continue;
            }
            let extent_type = item_data[20];
            match extent_type {
                EXTENT_INLINE => {
                    data.extend_from_slice(&item_data[21..]);
                }
                _ => {
                    if item_data.len() < 53 {
                        continue;
                    }
                    let disk_bytenr = u64::from_le_bytes(item_data[21..29].try_into().unwrap());
                    let num_bytes = u64::from_le_bytes(item_data[45..53].try_into().unwrap());
                    let absolute = self.volume_offset + disk_bytenr;
                    let mut buf = vec![0u8; num_bytes as usize];
                    let mut reader = self.reader.borrow_mut();
                    reader.seek(SeekFrom::Start(absolute))?;
                    reader.read_exact(&mut buf)?;
                    data.extend_from_slice(&buf);
                }
            }
        }
        Ok(truncate_data_to_declared_size(data, declared_size))
    }

    fn get_inode_size(&self, tree_root_bytenr: u64, inode_objectid: u64) -> io::Result<u64> {
        let key = BtrfsKey {
            objectid: inode_objectid,
            ty: INODE_ITEM_KEY,
            offset: 0,
        };
        let (leaf_data, items) = self.walk_to_leaf(tree_root_bytenr, &key)?;
        if let Ok(i) = items.binary_search_by(|item| item.key.cmp(&key)) {
            let idata = Self::get_item_data(&leaf_data, &items[i]);
            if idata.len() >= 24 {
                return Ok(u64::from_le_bytes(idata[16..24].try_into().unwrap()));
            }
        }
        Ok(0)
    }

    fn resolve_path_in_tree(
        &self,
        tree_root_bytenr: u64,
        root_dirid: u64,
        path: &str,
    ) -> io::Result<Option<(u64, bool, u64)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((root_dirid, true, 0)));
        }

        let mut current_dir = root_dirid;
        for (i, comp) in components.iter().enumerate() {
            let entries = self.list_dir_entries(tree_root_bytenr, current_dir)?;
            let is_last = i == components.len() - 1;
            let found = entries.iter().find(|(name, _, _)| name == comp);
            match found {
                Some((_, inode_obj, file_type)) => {
                    let is_dir = *file_type == FT_DIR;
                    if is_last {
                        let size = self.get_inode_size(tree_root_bytenr, *inode_obj)?;
                        return Ok(Some((*inode_obj, is_dir, size)));
                    }
                    if !is_dir {
                        return Ok(None);
                    }
                    current_dir = *inode_obj;
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// FileSystemReader impl
// ---------------------------------------------------------------------------

impl FileSystemReader for BtrfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        // Empty/root path: list subvolumes.
        if path.is_empty() || path == "/" || path == "\\" {
            let nodes: Vec<FsNode> = self
                .subvolumes
                .iter()
                .map(|sv| fs_node(sv.name.clone(), true, 0, None, None, None))
                .collect();
            return Ok(child_nodes_with_parent_path(nodes, ""));
        }

        // Split "subvol_name" or "subvol_name/dir1/dir2".
        let first_slash = path.find(['/', '\\']);
        let (subvol_name, sub_path) = match first_slash {
            Some(pos) => (&path[..pos], &path[pos + 1..]),
            None => (path, ""),
        };

        let subvol = self
            .subvolumes
            .iter()
            .find(|s| s.name == subvol_name)
            .ok_or_else(|| path_not_found(path))?;

        let (inode_obj, is_dir, _) = self
            .resolve_path_in_tree(subvol.tree_root_bytenr, subvol.root_dirid, sub_path)?
            .ok_or_else(|| path_not_found(path))?;

        if !is_dir {
            return Err(evidence_core::filesystem::path_is_not_directory(path));
        }

        let entries = self.list_dir_entries(subvol.tree_root_bytenr, inode_obj)?;
        let mut nodes = Vec::new();
        for (name, child_obj, file_type) in entries {
            if is_special_directory_name(&name) {
                continue;
            }
            let child_is_dir = file_type == FT_DIR;
            let size = if !child_is_dir {
                self.get_inode_size(subvol.tree_root_bytenr, child_obj)
                    .unwrap_or(0)
            } else {
                0
            };
            nodes.push(fs_node(name, child_is_dir, size, None, None, None));
        }
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let first_slash = path.find(['/', '\\']);
        let (subvol_name, sub_path) = match first_slash {
            Some(pos) => (&path[..pos], &path[pos + 1..]),
            None => return Err(file_not_found(path)),
        };

        let subvol = self
            .subvolumes
            .iter()
            .find(|s| s.name == subvol_name)
            .ok_or_else(|| file_not_found(path))?;

        let (inode_obj, is_dir, file_size) = self
            .resolve_path_in_tree(subvol.tree_root_bytenr, subvol.root_dirid, sub_path)?
            .ok_or_else(|| file_not_found(path))?;

        if is_dir {
            return Err(path_is_directory(path));
        }

        if file_size == 0 {
            return Ok(Box::new(io::Cursor::new(Vec::new())));
        }

        let data = self.read_file_extents(subvol.tree_root_bytenr, inode_obj, file_size)?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn data_source_name(&self) -> &str {
        "btrfs"
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use evidence_core::ReaderInfo;
    use std::io::{Read, Seek};

    // -------------------------------------------------------------------
    // Fake evidence reader
    // -------------------------------------------------------------------

    struct FakeReader {
        data: Vec<u8>,
        pos: u64,
    }

    impl FakeReader {
        fn new(data: Vec<u8>) -> Self {
            Self { data, pos: 0 }
        }
    }

    impl Read for FakeReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let start = (self.pos as usize).min(self.data.len());
            let end = (start + buf.len()).min(self.data.len());
            let n = end - start;
            buf[..n].copy_from_slice(&self.data[start..end]);
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl Seek for FakeReader {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.pos = match pos {
                SeekFrom::Start(p) => p,
                SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
                SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
            };
            Ok(self.pos)
        }
    }

    impl EvidenceReader for FakeReader {
        fn info(&self) -> &ReaderInfo {
            unimplemented!()
        }
    }

    // -------------------------------------------------------------------
    // Minimal Btrfs fixture
    // -------------------------------------------------------------------
    //
    // Layout (nodesize = 4096, 24 blocks = 0x18000 bytes):
    //
    //  Block  Offset    Logical   Content
    //  -----  --------  --------  --------------------------
    //   0-15  0x00000   --        Reserved (first 64K)
    //   16    0x10000   0x10000   Superblock
    //   17    0x11000   0x11000   Root tree internal node
    //   18    0x12000   0x12000   Root tree leaf: ROOT_ITEM
    //   19    0x13000   0x13000   FS tree leaf: dir + inode
    //   20    0x14000   0x14000   File data "Hello from Btrfs!"
    //   21    0x15000   0x15000   Nested file leaf (INODE+inline)

    fn build_btrfs_fixture() -> Vec<u8> {
        let nodesize: u64 = 4096;
        let total_blocks: u64 = 24;
        let total_size = (total_blocks * nodesize) as usize;
        let mut img = vec![0u8; total_size];

        let block = |n: u64| -> usize { (n * nodesize) as usize };

        // ---- Superblock at block 16 (0x10000) ----
        let sb = &mut img[block(16)..block(17)];
        sb[0x40..0x48].copy_from_slice(BTRFS_MAGIC);
        let root_tree_bytenr: u64 = 0x11000;
        sb[0x78..0x80].copy_from_slice(&root_tree_bytenr.to_le_bytes());
        sb[0x80..0x88].copy_from_slice(&root_tree_bytenr.to_le_bytes());
        sb[0xB8..0xBC].copy_from_slice(&4096u32.to_le_bytes());
        sb[0xBC..0xC0].copy_from_slice(&4096u32.to_le_bytes());
        sb[0xC0..0xC4].copy_from_slice(&4096u32.to_le_bytes());
        sb[0xC4..0xC8].copy_from_slice(&4096u32.to_le_bytes());

        // Sys chunk array: identity mapping for the entire image.
        let ca = &mut sb[0x32B..0x32B + 256];
        ca[0x00..0x08].copy_from_slice(&CHUNK_TREE_OBJECTID.to_le_bytes());
        ca[0x08] = CHUNK_ITEM_KEY;
        ca[0x09..0x11].copy_from_slice(&0u64.to_le_bytes());
        ca[0x11..0x19].copy_from_slice(&(total_blocks * nodesize).to_le_bytes());
        ca[0x19..0x21].copy_from_slice(&CHUNK_TREE_OBJECTID.to_le_bytes());
        ca[0x21..0x29].copy_from_slice(&nodesize.to_le_bytes());
        ca[0x29..0x31].copy_from_slice(&(1u64 | (1 << 2)).to_le_bytes());
        ca[0x31..0x35].copy_from_slice(&4096u32.to_le_bytes());
        ca[0x35..0x39].copy_from_slice(&4096u32.to_le_bytes());
        ca[0x39..0x3D].copy_from_slice(&4096u32.to_le_bytes());
        ca[0x3D..0x3F].copy_from_slice(&1u16.to_le_bytes());
        ca[0x3F..0x41].copy_from_slice(&1u16.to_le_bytes());
        ca[0x41..0x49].copy_from_slice(&1u64.to_le_bytes());
        ca[0x49..0x51].copy_from_slice(&0u64.to_le_bytes());
        let array_size: u32 = 0x51 + ((4 - (0x51 % 4)) % 4);
        sb[0xC8..0xCC].copy_from_slice(&array_size.to_le_bytes());

        // ---- Root tree internal node at block 17 (0x11000) ----
        let rt = &mut img[block(17)..block(18)];
        rt[0x30..0x38].copy_from_slice(&0x11000u64.to_le_bytes());
        rt[0x5D..0x61].copy_from_slice(&1u32.to_le_bytes());
        rt[0x61] = 1;
        let io = BTRFS_HEADER_SIZE;
        rt[io..io + 8].copy_from_slice(&FS_TREE_OBJECTID.to_le_bytes());
        rt[io + 8] = ROOT_ITEM_KEY;
        rt[io + 9..io + 17].copy_from_slice(&0u64.to_le_bytes());
        rt[io + 17..io + 25].copy_from_slice(&0x12000u64.to_le_bytes());
        rt[io + 25..io + 33].copy_from_slice(&1u64.to_le_bytes());

        // ---- Root tree leaf at block 18 (0x12000) ----
        let rtl = &mut img[block(18)..block(19)];
        rtl[0x30..0x38].copy_from_slice(&0x12000u64.to_le_bytes());
        rtl[0x5D..0x61].copy_from_slice(&2u32.to_le_bytes());
        rtl[0x61] = 0;

        let data_end = nodesize as usize;
        let mut doff = data_end;

        // Item 0: ROOT_ITEM (5,132,0)
        let ri_size = 244usize;
        doff -= ri_size;
        let k0 = BTRFS_HEADER_SIZE;
        rtl[k0..k0 + 8].copy_from_slice(&FS_TREE_OBJECTID.to_le_bytes());
        rtl[k0 + 8] = ROOT_ITEM_KEY;
        rtl[k0 + 9..k0 + 17].copy_from_slice(&0u64.to_le_bytes());
        rtl[k0 + 17..k0 + 21].copy_from_slice(&(doff as u32).to_le_bytes());
        rtl[k0 + 21..k0 + 25].copy_from_slice(&(ri_size as u32).to_le_bytes());
        let rid = &mut rtl[doff..doff + ri_size];
        rid[0..8].copy_from_slice(&1u64.to_le_bytes());
        rid[40..44].copy_from_slice(&1u32.to_le_bytes());
        rid[52..56].copy_from_slice(&S_IFDIR.to_le_bytes());
        rid[160..168].copy_from_slice(&1u64.to_le_bytes());
        rid[168..176].copy_from_slice(&FIRST_FREE_OBJECTID.to_le_bytes());
        rid[176..184].copy_from_slice(&0x13000u64.to_le_bytes());
        rid[184..192].copy_from_slice(&0u64.to_le_bytes());
        rid[192..200].copy_from_slice(&nodesize.to_le_bytes());
        rid[216..220].copy_from_slice(&1u32.to_le_bytes());

        // Item 1: ROOT_BACKREF (5,144,0)
        let rb_name = b"default";
        let rb_size = 18 + rb_name.len();
        doff -= rb_size;
        let k1 = BTRFS_HEADER_SIZE + 25;
        rtl[k1..k1 + 8].copy_from_slice(&FS_TREE_OBJECTID.to_le_bytes());
        rtl[k1 + 8] = ROOT_BACKREF_KEY;
        rtl[k1 + 9..k1 + 17].copy_from_slice(&0u64.to_le_bytes());
        rtl[k1 + 17..k1 + 21].copy_from_slice(&(doff as u32).to_le_bytes());
        rtl[k1 + 21..k1 + 25].copy_from_slice(&(rb_size as u32).to_le_bytes());
        let rbd = &mut rtl[doff..doff + rb_size];
        rbd[0..8].copy_from_slice(&FIRST_FREE_OBJECTID.to_le_bytes());
        rbd[8..16].copy_from_slice(&0u64.to_le_bytes());
        rbd[16..18].copy_from_slice(&(rb_name.len() as u16).to_le_bytes());
        rbd[18..18 + rb_name.len()].copy_from_slice(rb_name);

        // ---- FS tree leaf at block 19 (0x13000) ----
        let fs = &mut img[block(19)..block(20)];
        fs[0x30..0x38].copy_from_slice(&0x13000u64.to_le_bytes());
        fs[0x61] = 0;
        let fs_data_end = nodesize as usize;
        let mut fs_doff = fs_data_end;

        let file_content = b"Hello from Btrfs!";

        // Helper to write one leaf item descriptor + data.
        fn put_item(
            leaf: &mut [u8],
            idx: usize,
            key_obj: u64,
            key_type: u8,
            key_off: u64,
            data_bytes: &[u8],
            data_off: &mut usize,
        ) {
            let kbase = BTRFS_HEADER_SIZE + idx * LEAF_ITEM_SIZE;
            leaf[kbase..kbase + 8].copy_from_slice(&key_obj.to_le_bytes());
            leaf[kbase + 8] = key_type;
            leaf[kbase + 9..kbase + 17].copy_from_slice(&key_off.to_le_bytes());
            *data_off -= data_bytes.len();
            leaf[kbase + 17..kbase + 21].copy_from_slice(&(*data_off as u32).to_le_bytes());
            leaf[kbase + 21..kbase + 25].copy_from_slice(&(data_bytes.len() as u32).to_le_bytes());
            leaf[*data_off..*data_off + data_bytes.len()].copy_from_slice(data_bytes);
        }

        fn make_inode(mode: u32, size: u64, nlink: u32) -> Vec<u8> {
            let mut d = vec![0u8; 160];
            d[0..8].copy_from_slice(&1u64.to_le_bytes());
            d[16..24].copy_from_slice(&size.to_le_bytes());
            d[40..44].copy_from_slice(&nlink.to_le_bytes());
            d[52..56].copy_from_slice(&mode.to_le_bytes());
            d
        }

        fn make_dir_entry(name: &[u8], child_obj: u64, file_type: u8) -> Vec<u8> {
            let mut d = vec![0u8; 30 + name.len()];
            d[0..8].copy_from_slice(&child_obj.to_le_bytes());
            d[17..25].copy_from_slice(&1u64.to_le_bytes());
            d[27..29].copy_from_slice(&(name.len() as u16).to_le_bytes());
            d[29] = file_type;
            d[30..30 + name.len()].copy_from_slice(name);
            d
        }

        fn make_regular_extent(disk_bytenr: u64, ram_bytes: u64, num_bytes: u64) -> Vec<u8> {
            let mut d = vec![0u8; 53];
            d[0..8].copy_from_slice(&1u64.to_le_bytes());
            d[8..16].copy_from_slice(&ram_bytes.to_le_bytes());
            d[20] = EXTENT_REGULAR;
            d[21..29].copy_from_slice(&disk_bytenr.to_le_bytes());
            d[29..37].copy_from_slice(&4096u64.to_le_bytes());
            d[37..45].copy_from_slice(&0u64.to_le_bytes());
            d[45..53].copy_from_slice(&num_bytes.to_le_bytes());
            d
        }

        // Item 0: INODE_ITEM (256) - root dir
        put_item(
            fs,
            0,
            256,
            INODE_ITEM_KEY,
            0,
            &make_inode(S_IFDIR, 0, 3),
            &mut fs_doff,
        );

        // Item 1: DIR_INDEX "file.txt" (child 257)
        put_item(
            fs,
            1,
            256,
            DIR_INDEX_KEY,
            1,
            &make_dir_entry(b"file.txt", 257, FT_REG_FILE),
            &mut fs_doff,
        );

        // Item 2: DIR_INDEX "subdir" (child 258)
        put_item(
            fs,
            2,
            256,
            DIR_INDEX_KEY,
            2,
            &make_dir_entry(b"subdir", 258, FT_DIR),
            &mut fs_doff,
        );

        // Item 3: INODE_ITEM (257) - file.txt
        put_item(
            fs,
            3,
            257,
            INODE_ITEM_KEY,
            0,
            &make_inode(S_IFREG, file_content.len() as u64, 1),
            &mut fs_doff,
        );

        // Item 4: EXTENT_DATA (257,0) - regular extent at block 20
        put_item(
            fs,
            4,
            257,
            EXTENT_DATA_KEY,
            0,
            &make_regular_extent(
                0x14000,
                file_content.len() as u64,
                file_content.len() as u64,
            ),
            &mut fs_doff,
        );

        // Item 5: INODE_ITEM (258) - subdir
        put_item(
            fs,
            5,
            258,
            INODE_ITEM_KEY,
            0,
            &make_inode(S_IFDIR, 0, 2),
            &mut fs_doff,
        );

        // Item 6: DIR_INDEX "nested.dat" in subdir (parent 258)
        put_item(
            fs,
            6,
            258,
            DIR_INDEX_KEY,
            1,
            &make_dir_entry(b"nested.dat", 259, FT_REG_FILE),
            &mut fs_doff,
        );

        let nested_content = b"Nested file data";

        // Item 7: INODE_ITEM (259)
        put_item(
            fs,
            7,
            259,
            INODE_ITEM_KEY,
            0,
            &make_inode(S_IFREG, nested_content.len() as u64, 1),
            &mut fs_doff,
        );

        // Item 8: EXTENT_DATA inline for nested.dat
        let mut inline_ext = vec![0u8; 21 + nested_content.len()];
        inline_ext[0..8].copy_from_slice(&1u64.to_le_bytes());
        inline_ext[8..16].copy_from_slice(&(nested_content.len() as u64).to_le_bytes());
        inline_ext[20] = EXTENT_INLINE;
        inline_ext[21..21 + nested_content.len()].copy_from_slice(nested_content);
        put_item(fs, 8, 259, EXTENT_DATA_KEY, 0, &inline_ext, &mut fs_doff);

        fs[0x5D..0x61].copy_from_slice(&9u32.to_le_bytes());

        // ---- Block 20: file.txt data ----
        img[block(20)..block(20) + file_content.len()].copy_from_slice(file_content);

        img
    }

    // -------------------------------------------------------------------
    // test_superblock_magic
    // -------------------------------------------------------------------

    #[test]
    fn test_superblock_magic() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();
        assert_eq!(btrfs.data_source_name(), "btrfs");
        assert_eq!(btrfs.sectorsize, 4096);
        assert_eq!(btrfs.nodesize, 4096);
    }

    // -------------------------------------------------------------------
    // test_chunk_mapping
    // -------------------------------------------------------------------

    #[test]
    fn test_chunk_mapping() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        assert!(!btrfs.chunk_map.is_empty());
        let phys = btrfs.translate_logical(0x10000).unwrap();
        assert_eq!(phys, 0x10000);
    }

    // -------------------------------------------------------------------
    // test_subvolume_listing
    // -------------------------------------------------------------------

    #[test]
    fn test_subvolume_listing() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        assert!(!btrfs.subvolumes.is_empty());
        let sv = btrfs
            .subvolumes
            .iter()
            .find(|s| s.name == "default")
            .expect("should find 'default' subvolume");
        assert_eq!(sv.id, FS_TREE_OBJECTID);
        assert_eq!(sv.root_dirid, FIRST_FREE_OBJECTID);
        assert!(sv.tree_root_bytenr > 0);
    }

    // -------------------------------------------------------------------
    // test_root_directory_listing
    // -------------------------------------------------------------------

    #[test]
    fn test_root_directory_listing() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        let root = btrfs.root().unwrap();
        assert_eq!(root.name, "\\");
        assert!(root.is_dir);

        let top = btrfs.list_children("").unwrap();
        let top_names: Vec<&str> = top.iter().map(|n| n.name.as_str()).collect();
        assert!(top_names.contains(&"default"));

        let sv = btrfs.list_children("default").unwrap();
        let sv_names: Vec<&str> = sv.iter().map(|n| n.name.as_str()).collect();
        assert!(sv_names.contains(&"file.txt"));
        assert!(sv_names.contains(&"subdir"));

        let file = sv.iter().find(|n| n.name == "file.txt").unwrap();
        assert!(!file.is_dir);
        assert_eq!(file.size, 17);

        let dir = sv.iter().find(|n| n.name == "subdir").unwrap();
        assert!(dir.is_dir);
    }

    // -------------------------------------------------------------------
    // test_file_read
    // -------------------------------------------------------------------

    #[test]
    fn test_file_read() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        let mut f = btrfs.open_file("default/file.txt").unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Hello from Btrfs!");
    }

    // -------------------------------------------------------------------
    // test_nested_file_read
    // -------------------------------------------------------------------

    #[test]
    fn test_nested_file_read() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        let mut f = btrfs.open_file("default/subdir/nested.dat").unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Nested file data");
    }

    // -------------------------------------------------------------------
    // test_invalid_magic_rejected
    // -------------------------------------------------------------------

    #[test]
    fn test_invalid_magic_rejected() {
        let mut img = build_btrfs_fixture();
        let sb_off = BTRFS_SUPERBLOCK_OFFSET as usize;
        img[sb_off + 0x40] = 0x00;
        img[sb_off + 0x41] = 0x00;

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        match BtrfsReader::open(reader, 0) {
            Ok(_) => panic!("expected error"),
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::InvalidData);
                assert!(e.to_string().contains("magic"));
            }
        }
    }

    // -------------------------------------------------------------------
    // test_nonexistent_path
    // -------------------------------------------------------------------

    #[test]
    fn test_nonexistent_path() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        let e = btrfs.list_children("nonexistent").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);

        let e = match btrfs.open_file("default/no_such.txt") {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }

    // -------------------------------------------------------------------
    // test_subvolume_count
    // -------------------------------------------------------------------

    #[test]
    fn test_subvolume_count() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();
        assert!(
            !btrfs.subvolumes.is_empty(),
            "should have at least one subvolume"
        );
        assert!(btrfs.subvolumes.iter().any(|s| s.name == "default"));
    }

    // -------------------------------------------------------------------
    // test_chunk_identity_mapping
    // -------------------------------------------------------------------

    #[test]
    fn test_chunk_identity_mapping() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        // An address outside the single chunk (0..0x18000) triggers the
        // fallback identity mapping.
        let phys = btrfs.translate_logical(0x20000).unwrap();
        assert_eq!(phys, 0x20000);
    }

    // -------------------------------------------------------------------
    // test_subdir_listing
    // -------------------------------------------------------------------

    #[test]
    fn test_subdir_listing() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        let children = btrfs.list_children("default/subdir").unwrap();
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(
            names.contains(&"nested.dat"),
            "subdir should contain nested.dat"
        );
    }

    // -------------------------------------------------------------------
    // test_superblock_values
    // -------------------------------------------------------------------

    #[test]
    fn test_superblock_values() {
        let img = build_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();
        assert!(btrfs.sectorsize > 0, "sectorsize must be > 0");
        assert!(btrfs.nodesize > 0, "nodesize must be > 0");
    }
}
