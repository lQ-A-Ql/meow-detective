//! Checking a filesystem without changing it (#260).
//!
//! What `fsck.btrfs` runs: a walk of every tree the volume declares, with
//! each structure checked against the others that describe the same
//! thing. Nothing is written and nothing is repaired. A volume this calls
//! clean is one on which none of the invariants below is broken; it is a
//! subset of what `btrfs check --readonly` checks, and says so.
//!
//! # What is checked
//!
//! - **The superblock copies** agree with the primary on what the
//!   filesystem is.
//! - **Every tree block** of every tree is read, which checks its
//!   checksum, address and filesystem; its level is the one its parent
//!   expects, its generation the one its parent's pointer records, its
//!   keys are in order, and its first key is the one its parent files it
//!   under.
//! - **The extent tree against the trees**: every block a tree reaches has
//!   an extent item, every tree-block extent item is reached, and every
//!   file extent points at a data extent item.
//! - **Block groups**: each chunk has one, each one a chunk, and its
//!   `used` is what the extents in it add up to; the superblock's
//!   `bytes_used` is what the groups add up to.
//! - **Chunks against device extents**: every stripe of every chunk has
//!   its device extent, of the stripe's length, and every device extent
//!   belongs to a stripe; each device's `bytes_used` adds them up.
//! - **The free-space tree** agrees with what the extent tree leaves free.
//! - **Every subvolume's inodes and directories**: every item belongs to
//!   an inode that has an inode item; each directory entry has its index
//!   twin, names an inode or subvolume that exists, records its type, and
//!   is matched by the inode's back-reference; link counts and directory
//!   sizes are what the entries add up to.
//! - **The checksum tree** covers only bytes inside data extents.

use crate::chunk::DiskKey;
use crate::fs::Filesystem;
use fs_core::BlockRead;
use std::collections::{BTreeMap, HashMap, HashSet};

/// One thing found wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The tree it was found in, by objectid, where it belongs to one.
    pub tree: Option<u64>,
    /// What is wrong, in words.
    pub what: String,
}

/// What a check found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Everything found wrong, in the order it was found.
    pub findings: Vec<Finding>,
    /// Tree blocks read.
    pub tree_blocks: u64,
    /// Inodes walked, across every subvolume.
    pub inodes: u64,
    /// Subvolumes walked, the default one included.
    pub subvolumes: u64,
}

impl Report {
    /// True when nothing was found wrong.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// The most findings reported of one kind, so a wholesale corruption does
/// not print a million lines.
const PER_KIND: usize = 20;

mod key {
    pub const INODE_ITEM: u8 = 1;
    pub const INODE_REF: u8 = 12;
    pub const INODE_EXTREF: u8 = 13;
    pub const DIR_ITEM: u8 = 84;
    pub const DIR_INDEX: u8 = 96;
    pub const EXTENT_DATA: u8 = 108;
    pub const EXTENT_CSUM: u8 = 128;
    pub const ROOT_ITEM: u8 = 132;
    pub const EXTENT_ITEM: u8 = 168;
    pub const METADATA_ITEM: u8 = 169;
    pub const DEV_EXTENT: u8 = 204;
    pub const DEV_ITEM: u8 = 216;
    pub const CHUNK_ITEM: u8 = 228;
}

mod oid {
    pub const CHUNK_TREE: u64 = 3;
    pub const EXTENT_TREE: u64 = 2;
    pub const DEV_TREE: u64 = 4;
    pub const FS_TREE: u64 = 5;
    pub const CSUM_TREE: u64 = 7;
    pub const FIRST_FREE: u64 = 256;
    pub const LAST_FREE: u64 = u64::MAX - 256;
    pub const DATA_RELOC_TREE: u64 = u64::MAX - 8;
}

/// A subvolume tree: the default one, the data-relocation tree, and every
/// subvolume and snapshot.
fn is_fs_tree(id: u64) -> bool {
    id == oid::FS_TREE
        || id == oid::DATA_RELOC_TREE
        || (oid::FIRST_FREE..=oid::LAST_FREE).contains(&id)
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn le64(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 8)
        .map(|s| u64::from_le_bytes(s.try_into().expect("8 bytes")))
}

/// What one inode's items say about it, gathered from its tree.
#[derive(Default)]
struct InodeFacts {
    item: Option<crate::inode::Inode>,
    /// (parent directory, name) for every back-reference.
    names: Vec<(u64, Vec<u8>)>,
    /// For a directory: (name, target, location type, file type) per
    /// DIR_ITEM, and per DIR_INDEX.
    dir_items: Vec<(Vec<u8>, u64, u8, Option<crate::inode::FileType>)>,
    dir_index: Vec<(Vec<u8>, u64, u8, Option<crate::inode::FileType>)>,
}

struct Checker<'a> {
    fs: &'a Filesystem,
    report: Report,
    counted: HashMap<(Option<u64>, &'static str), usize>,
    /// Every tree block reached, and how many times.
    reached: HashMap<u64, u32>,
    /// Tree-block extent items: address to (refs).
    tree_extents: BTreeMap<u64, u64>,
    /// Data extent items: start to (length, refs).
    data_extents: BTreeMap<u64, (u64, u64)>,
    /// File extents' disk ranges, to check against data extents after.
    file_extents: Vec<(u64, u64, u64, u64)>, // (tree, ino, disk start, disk len)
    chunks: Vec<crate::chunk::Chunk>,
    dev_extents: BTreeMap<(u64, u64), (u64, u64)>, // (devid, physical) -> (chunk offset, length)
    dev_used: BTreeMap<u64, u64>,
    csums: Vec<(u64, u64)>, // (start, length)
    subvolume_ids: HashSet<u64>,
}

/// Check `fs`. Never writes.
pub fn check(fs: &Filesystem) -> Report {
    let mut c = Checker {
        fs,
        report: Report::default(),
        counted: HashMap::new(),
        reached: HashMap::new(),
        tree_extents: BTreeMap::new(),
        data_extents: BTreeMap::new(),
        file_extents: Vec::new(),
        chunks: Vec::new(),
        dev_extents: BTreeMap::new(),
        dev_used: BTreeMap::new(),
        csums: Vec::new(),
        subvolume_ids: HashSet::new(),
    };
    c.superblock_copies();
    c.trees();
    c.extents_against_trees();
    c.block_groups();
    c.chunks_against_device_extents();
    c.free_space();
    c.checksums();
    c.report
}

impl Checker<'_> {
    fn find(&mut self, kind: &'static str, tree: Option<u64>, what: String) {
        let n = self.counted.entry((tree, kind)).or_insert(0);
        *n += 1;
        if *n <= PER_KIND {
            self.report.findings.push(Finding { tree, what });
        } else if *n == PER_KIND + 1 {
            self.report.findings.push(Finding {
                tree,
                what: format!("more findings of this kind ({kind}) are not listed"),
            });
        }
    }

    fn superblock_copies(&mut self) {
        let sb = self.fs.superblock().clone();
        let size = self.fs.device.size_bytes();
        for at in [64u64 << 20, 256u64 << 30] {
            if at + 4096 > size {
                continue;
            }
            let mut raw = vec![0u8; 4096];
            if let Err(e) = self.fs.device.read_at(at, &mut raw) {
                self.find(
                    "superblock",
                    None,
                    format!("reading the superblock copy at {at}: {e}"),
                );
                continue;
            }
            let copy = match crate::superblock::Superblock::parse(&raw) {
                Ok(copy) => copy,
                Err(e) => {
                    self.find(
                        "superblock",
                        None,
                        format!("the superblock copy at {at}: {e}"),
                    );
                    continue;
                }
            };
            let fields: [(&str, u64, u64); 5] = [
                ("total_bytes", sb.total_bytes, copy.total_bytes),
                ("num_devices", sb.num_devices, copy.num_devices),
                ("nodesize", sb.nodesize.into(), copy.nodesize.into()),
                ("sectorsize", sb.sectorsize.into(), copy.sectorsize.into()),
                (
                    "csum_type",
                    sb.csum_type.to_raw().into(),
                    copy.csum_type.to_raw().into(),
                ),
            ];
            for (name, primary, secondary) in fields {
                if primary != secondary {
                    self.find(
                        "superblock",
                        None,
                        format!("the superblock copy at {at} says {name} {secondary}; the primary says {primary}"),
                    );
                }
            }
            if copy.fsid != sb.fsid {
                self.find(
                    "superblock",
                    None,
                    format!("the superblock copy at {at} has another filesystem's id"),
                );
            }
        }
    }

    /// Walk the chunk tree, the root tree, and every tree the root tree
    /// names.
    fn trees(&mut self) {
        let sb = self.fs.superblock().clone();
        self.walk(oid::CHUNK_TREE, sb.chunk_root, sb.chunk_root_level);
        self.walk(1, sb.root, sb.root_level);
        let roots = match self.fs.root_tree_items() {
            Ok(items) => items,
            Err(e) => {
                self.find("tree", Some(1), format!("the root tree: {e}"));
                return;
            }
        };
        let mut seen = HashSet::new();
        for (objectid, key_type, _offset, data) in roots {
            if key_type != key::ROOT_ITEM {
                continue;
            }
            let (Some(bytenr), Some(&level)) = (le64(&data, 176), data.get(238)) else {
                self.find(
                    "tree",
                    Some(1),
                    format!("tree {objectid}'s root item is short"),
                );
                continue;
            };
            if is_fs_tree(objectid) {
                self.subvolume_ids.insert(objectid);
            }
            if !seen.insert(bytenr) {
                continue;
            }
            self.walk(objectid, bytenr, level);
        }
        let fs_trees: Vec<u64> = self.subvolume_ids.iter().copied().collect();
        for id in fs_trees {
            self.report.subvolumes += 1;
            self.namespace(id);
        }
    }

    fn walk(&mut self, tree: u64, root: u64, level: u8) {
        // (block, the level the parent expects, the generation its
        // pointer records, the key its parent files it under)
        let mut stack: Vec<(u64, u8, Option<u64>, Option<DiskKey>)> =
            vec![(root, level, None, None)];
        while let Some((logical, want_level, want_gen, want_key)) = stack.pop() {
            let count = self.reached.entry(logical).or_insert(0);
            *count += 1;
            if *count > 1 {
                // Shared with another tree (a snapshot): walked already.
                continue;
            }
            let block = match self.fs.read_tree_block(logical) {
                Ok(b) => b,
                Err(e) => {
                    self.find(
                        "block",
                        Some(tree),
                        format!("tree {tree}: the block at {logical}: {e}"),
                    );
                    continue;
                }
            };
            self.report.tree_blocks += 1;
            let h = &block.header;
            if h.level != want_level {
                self.find(
                    "block",
                    Some(tree),
                    format!("tree {tree}: the block at {logical} is level {}; its parent expects {want_level}", h.level),
                );
            }
            if let Some(g) = want_gen {
                if h.generation != g {
                    self.find(
                        "block",
                        Some(tree),
                        format!(
                            "tree {tree}: the block at {logical} is generation {}; its parent's pointer records {g}",
                            h.generation
                        ),
                    );
                }
            }
            let keys: Vec<DiskKey> = match &block.body {
                crate::btree::Body::Node(ptrs) => ptrs.iter().map(|p| p.key).collect(),
                crate::btree::Body::Leaf(items) => items.iter().map(|i| i.key).collect(),
            };
            if let (Some(want), Some(first)) = (want_key, keys.first()) {
                if *first != want {
                    self.find(
                        "block",
                        Some(tree),
                        format!("tree {tree}: the block at {logical} starts at {first:?}; its parent files it under {want:?}"),
                    );
                }
            }
            for pair in keys.windows(2) {
                if crate::btree::compare_keys(&pair[0], &pair[1]) != std::cmp::Ordering::Less {
                    self.find(
                        "block",
                        Some(tree),
                        format!(
                            "tree {tree}: the block at {logical} has keys out of order at {:?}",
                            pair[1]
                        ),
                    );
                    break;
                }
            }
            match &block.body {
                crate::btree::Body::Node(ptrs) => {
                    if h.level == 0 {
                        continue;
                    }
                    for p in ptrs.iter().rev() {
                        stack.push((p.blockptr, h.level - 1, Some(p.generation), Some(p.key)));
                    }
                }
                crate::btree::Body::Leaf(items) => {
                    for item in items {
                        let Some(data) = block.item_data(item) else {
                            self.find(
                                "block",
                                Some(tree),
                                format!("tree {tree}: an item in the block at {logical} lies outside it"),
                            );
                            continue;
                        };
                        self.item(tree, &item.key, data);
                    }
                }
            }
        }
    }

    /// An item of a tree that is not a subvolume's: what it says about
    /// space.
    fn item(&mut self, tree: u64, k: &DiskKey, data: &[u8]) {
        match (tree, k.key_type) {
            (oid::EXTENT_TREE, key::METADATA_ITEM) => {
                self.tree_extents
                    .insert(k.objectid, le64(data, 0).unwrap_or(0));
            }
            (oid::EXTENT_TREE, key::EXTENT_ITEM) => {
                let refs = le64(data, 0).unwrap_or(0);
                let flags = le64(data, 16).unwrap_or(0);
                if flags & 2 != 0 {
                    self.tree_extents.insert(k.objectid, refs);
                } else {
                    self.data_extents.insert(k.objectid, (k.offset, refs));
                }
            }
            (oid::CHUNK_TREE, key::CHUNK_ITEM) => {
                match crate::chunk::Chunk::parse(k.offset, data) {
                    Ok(chunk) => self.chunks.push(chunk),
                    Err(e) => self.find(
                        "chunk",
                        Some(tree),
                        format!("the chunk at {}: {e}", k.offset),
                    ),
                }
            }
            (oid::CHUNK_TREE, key::DEV_ITEM) => {
                let devid = le64(data, 0).unwrap_or(0);
                let used = le64(data, 16).unwrap_or(0);
                self.dev_used.insert(devid, used);
            }
            (oid::DEV_TREE, key::DEV_EXTENT) => {
                let offset = le64(data, 16).unwrap_or(0);
                let len = le64(data, 24).unwrap_or(0);
                self.dev_extents
                    .insert((k.objectid, k.offset), (offset, len));
            }
            (oid::CSUM_TREE, key::EXTENT_CSUM) => {
                let size = self.fs.superblock().csum_type.digest_len() as u64;
                let sectors = data.len() as u64 / size.max(1);
                self.csums.push((
                    k.offset,
                    sectors * u64::from(self.fs.superblock().sectorsize),
                ));
            }
            _ => {}
        }
    }

    /// One subvolume's inodes and directories.
    fn namespace(&mut self, tree: u64) {
        let root = match self.fs.tree_root_public(tree) {
            Ok(r) => r,
            Err(e) => {
                self.find("namespace", Some(tree), format!("subvolume {tree}: {e}"));
                return;
            }
        };
        let mut inodes: BTreeMap<u64, InodeFacts> = BTreeMap::new();
        let mut file_extents = Vec::new();
        let mut bad: Vec<String> = Vec::new();
        let walked = self
            .fs
            .for_each_item_in(root, &mut |k: &DiskKey, data: &[u8]| {
                let ino = k.objectid;
                match k.key_type {
                    key::INODE_ITEM => match crate::inode::Inode::parse(data, ino) {
                        Ok(i) => inodes.entry(ino).or_default().item = Some(i),
                        Err(e) => bad.push(format!("inode {ino}: {e}")),
                    },
                    key::INODE_REF => {
                        let mut at = 0;
                        while at + 10 <= data.len() {
                            let len = usize::from(le16(data, at + 8).unwrap_or(0));
                            let name = data.get(at + 10..at + 10 + len).unwrap_or(&[]).to_vec();
                            inodes.entry(ino).or_default().names.push((k.offset, name));
                            at += 10 + len;
                        }
                    }
                    key::INODE_EXTREF => {
                        let mut at = 0;
                        while at + 18 <= data.len() {
                            let parent = le64(data, at).unwrap_or(0);
                            let len = usize::from(le16(data, at + 16).unwrap_or(0));
                            let name = data.get(at + 18..at + 18 + len).unwrap_or(&[]).to_vec();
                            inodes.entry(ino).or_default().names.push((parent, name));
                            at += 18 + len;
                        }
                    }
                    key::DIR_ITEM | key::DIR_INDEX => match crate::dir::parse_dir_items(data) {
                        Ok(entries) => {
                            let facts = inodes.entry(ino).or_default();
                            for e in entries {
                                let row = (e.name.clone(), e.ino, e.location_type, e.ftype);
                                if k.key_type == key::DIR_ITEM {
                                    facts.dir_items.push(row);
                                } else {
                                    facts.dir_index.push(row);
                                }
                            }
                        }
                        Err(e) => bad.push(format!("directory {ino}: {e}")),
                    },
                    key::EXTENT_DATA => {
                        // Regular or preallocated, with an address: a range of a
                        // data extent. Inline extents and holes have none.
                        let kind = data.get(20).copied().unwrap_or(0);
                        let disk = le64(data, 21).unwrap_or(0);
                        let disk_len = le64(data, 29).unwrap_or(0);
                        if (kind == 1 || kind == 2) && disk != 0 {
                            file_extents.push((ino, disk, disk_len));
                        }
                        inodes.entry(ino).or_default();
                    }
                    _ => {}
                }
            });
        if let Err(e) = walked {
            self.find("namespace", Some(tree), format!("subvolume {tree}: {e}"));
            return;
        }
        for what in bad {
            self.find("inode", Some(tree), format!("subvolume {tree}: {what}"));
        }
        for (ino, disk, len) in file_extents {
            self.file_extents.push((tree, ino, disk, len));
        }

        for (&ino, facts) in &inodes {
            let Some(item) = &facts.item else {
                self.find(
                    "inode",
                    Some(tree),
                    format!("subvolume {tree}: inode {ino} has items and no inode item"),
                );
                continue;
            };
            self.report.inodes += 1;
            if item.is_dir() {
                if item.nlink != 1 {
                    self.find(
                        "link count",
                        Some(tree),
                        format!("subvolume {tree}: directory {ino} has a link count of {}; a directory's is 1", item.nlink),
                    );
                }
                let size: u64 = facts.dir_index.iter().map(|e| 2 * e.0.len() as u64).sum();
                if item.size != size {
                    self.find(
                        "directory",
                        Some(tree),
                        format!("subvolume {tree}: directory {ino} records size {}; its entries add up to {size}", item.size),
                    );
                }
            } else if u64::from(item.nlink) != facts.names.len() as u64 {
                self.find(
                    "link count",
                    Some(tree),
                    format!(
                        "subvolume {tree}: inode {ino} has a link count of {}; {} names refer to it",
                        item.nlink,
                        facts.names.len()
                    ),
                );
            }
            // Every entry: its twin, its target, its type, its back-reference.
            let mut index: Vec<_> = facts.dir_index.iter().map(|e| (e.0.clone(), e.1)).collect();
            index.sort();
            let mut items: Vec<_> = facts.dir_items.iter().map(|e| (e.0.clone(), e.1)).collect();
            items.sort();
            if index != items {
                self.find(
                    "directory",
                    Some(tree),
                    format!(
                        "subvolume {tree}: directory {ino} has {} entries by name and {} by index, and they differ",
                        items.len(),
                        index.len()
                    ),
                );
            }
            for (name, target, location, ftype) in &facts.dir_items {
                let shown = String::from_utf8_lossy(name);
                if *location == key::ROOT_ITEM {
                    if !self.subvolume_ids.contains(target) {
                        self.find(
                            "directory",
                            Some(tree),
                            format!("subvolume {tree}: directory {ino}: {shown:?} names subvolume {target}, which does not exist"),
                        );
                    }
                    continue;
                }
                let Some(t) = inodes.get(target).and_then(|f| f.item.as_ref()) else {
                    self.find(
                        "directory",
                        Some(tree),
                        format!("subvolume {tree}: directory {ino}: {shown:?} names inode {target}, which does not exist"),
                    );
                    continue;
                };
                if ftype.is_some() && *ftype != t.file_type() {
                    self.find(
                        "directory",
                        Some(tree),
                        format!(
                            "subvolume {tree}: directory {ino}: {shown:?} says {ftype:?}; inode {target} is {:?}",
                            t.file_type()
                        ),
                    );
                }
                let backed = inodes
                    .get(target)
                    .is_some_and(|f| f.names.iter().any(|(p, n)| *p == ino && n == name));
                if !backed {
                    self.find(
                        "directory",
                        Some(tree),
                        format!("subvolume {tree}: directory {ino}: inode {target} has no back-reference for {shown:?}"),
                    );
                }
            }
        }
    }

    fn extents_against_trees(&mut self) {
        let reached: Vec<(u64, u32)> = self.reached.iter().map(|(&b, &n)| (b, n)).collect();
        for (block, times) in reached {
            match self.tree_extents.get(&block).copied() {
                None => self.find("extent", None, format!("the tree block at {block} has no extent item")),
                Some(refs) if u64::from(times) > refs.max(1) => self.find(
                    "extent",
                    None,
                    format!("the tree block at {block} is reached {times} times and its extent item counts {refs}"),
                ),
                Some(_) => {}
            }
        }
        let unreached: Vec<u64> = self
            .tree_extents
            .keys()
            .copied()
            .filter(|b| !self.reached.contains_key(b))
            .collect();
        for block in unreached {
            self.find(
                "extent",
                None,
                format!(
                    "the extent item for a tree block at {block} names a block no tree reaches"
                ),
            );
        }
        let mut referenced = HashSet::new();
        let file_extents = std::mem::take(&mut self.file_extents);
        for (tree, ino, disk, len) in &file_extents {
            match self.data_extents.get(disk) {
                Some((l, _)) if l == len => {
                    referenced.insert(*disk);
                }
                Some((l, _)) => self.find(
                    "extent",
                    Some(*tree),
                    format!("subvolume {tree}: inode {ino} references {len} bytes at {disk}; the data extent there is {l}"),
                ),
                None => self.find(
                    "extent",
                    Some(*tree),
                    format!("subvolume {tree}: inode {ino} references data at {disk} that no extent item holds"),
                ),
            }
        }
        let orphans: Vec<u64> = self
            .data_extents
            .keys()
            .copied()
            .filter(|d| !referenced.contains(d))
            .collect();
        for disk in orphans {
            self.find(
                "extent",
                None,
                format!("the data extent at {disk} is referenced by no file"),
            );
        }
        self.file_extents = file_extents;
    }

    fn block_groups(&mut self) {
        let groups = match self.fs.block_groups() {
            Ok(g) => g,
            Err(e) => {
                self.find("block group", None, format!("the block groups: {e}"));
                return;
            }
        };
        let nodesize = u64::from(self.fs.superblock().nodesize);
        let mut total = 0u64;
        for g in &groups {
            let tree: u64 =
                self.tree_extents.range(g.start..g.start + g.length).count() as u64 * nodesize;
            let data: u64 = self
                .data_extents
                .range(g.start..g.start + g.length)
                .map(|(_, (len, _))| *len)
                .sum();
            if g.used != tree + data {
                self.find(
                    "block group",
                    None,
                    format!(
                        "the block group at {} records {} used; its extents add up to {}",
                        g.start,
                        g.used,
                        tree + data
                    ),
                );
            }
            total += g.used;
            if !self
                .chunks
                .iter()
                .any(|c| c.logical == g.start && c.length == g.length)
            {
                self.find(
                    "block group",
                    None,
                    format!("the block group at {} has no chunk", g.start),
                );
            }
        }
        for c in self.chunks.clone() {
            if !groups
                .iter()
                .any(|g| g.start == c.logical && g.length == c.length)
            {
                self.find(
                    "block group",
                    None,
                    format!("the chunk at {} has no block group", c.logical),
                );
            }
        }
        let said = self.fs.superblock().bytes_used;
        if said != total {
            self.find(
                "counter",
                None,
                format!(
                    "the superblock says {said} bytes used; the block groups add up to {total}"
                ),
            );
        }
    }

    fn chunks_against_device_extents(&mut self) {
        const RAID0: u64 = 1 << 3;
        const RAID10: u64 = 1 << 6;
        let mut claimed = HashSet::new();
        for c in self.chunks.clone() {
            let stripes = u64::from(c.num_stripes.max(1));
            let per_stripe = if c.chunk_type & RAID0 != 0 {
                c.length / stripes
            } else if c.chunk_type & RAID10 != 0 {
                c.length / (stripes / u64::from(c.sub_stripes.max(1))).max(1)
            } else {
                c.length
            };
            for s in &c.stripes {
                claimed.insert((s.devid, s.offset));
                match self.dev_extents.get(&(s.devid, s.offset)).copied() {
                    Some((offset, len)) if offset == c.logical && len == per_stripe => {}
                    Some((offset, len)) => self.find(
                        "device extent",
                        None,
                        format!(
                            "the device extent at {} on device {} says chunk {offset}, {len} bytes; the chunk at {} \
                             expects {per_stripe}",
                            s.offset, s.devid, c.logical
                        ),
                    ),
                    None => self.find(
                        "device extent",
                        None,
                        format!("the chunk at {} has a stripe at {} on device {} with no device extent", c.logical, s.offset, s.devid),
                    ),
                }
            }
        }
        let mut per_dev: BTreeMap<u64, u64> = BTreeMap::new();
        for (&(devid, phys), &(_, len)) in &self.dev_extents.clone() {
            *per_dev.entry(devid).or_insert(0) += len;
            if !claimed.contains(&(devid, phys)) {
                self.find(
                    "device extent",
                    None,
                    format!("the device extent at {phys} on device {devid} belongs to no chunk"),
                );
            }
        }
        for (devid, said) in self.dev_used.clone() {
            let sum = per_dev.get(&devid).copied().unwrap_or(0);
            if said != sum {
                self.find(
                    "counter",
                    None,
                    format!("device {devid} records {said} bytes used; its device extents add up to {sum}"),
                );
            }
        }
    }

    fn free_space(&mut self) {
        let Ok(groups) = self.fs.block_groups() else {
            return;
        };
        for g in &groups {
            let cached = match self.fs.cached_free_extents(g) {
                Ok(Some(c)) => c,
                Ok(None) => continue,
                Err(e) => {
                    self.find(
                        "free space",
                        None,
                        format!("the free-space tree for the group at {}: {e}", g.start),
                    );
                    continue;
                }
            };
            let computed = match self.fs.free_extents(g) {
                Ok(c) => c,
                Err(e) => {
                    self.find(
                        "free space",
                        None,
                        format!("the extents in the group at {}: {e}", g.start),
                    );
                    continue;
                }
            };
            let a: Vec<(u64, u64)> = crate::block_group::merge_adjacent(cached)
                .iter()
                .map(|e| (e.start, e.len))
                .collect();
            let b: Vec<(u64, u64)> = crate::block_group::merge_adjacent(computed)
                .iter()
                .map(|e| (e.start, e.len))
                .collect();
            if a != b {
                self.find(
                    "free space",
                    None,
                    format!(
                        "the free-space tree says the group at {} has {} bytes free in {} runs; the extent tree leaves {} in {}",
                        g.start,
                        a.iter().map(|r| r.1).sum::<u64>(),
                        a.len(),
                        b.iter().map(|r| r.1).sum::<u64>(),
                        b.len()
                    ),
                );
            }
        }
    }

    fn checksums(&mut self) {
        for (start, len) in self.csums.clone() {
            // One checksum item may run across several adjacent extents,
            // so what is checked is that the extents cover it together.
            let mut covered = start;
            while covered < start + len {
                match self.data_extents.range(..=covered).next_back() {
                    Some((&s, &(l, _))) if covered < s + l => covered = s + l,
                    _ => break,
                }
            }
            if covered < start + len {
                self.find(
                    "checksum",
                    Some(oid::CSUM_TREE),
                    format!(
                        "checksums for {len} bytes at {start} cover bytes no data extent holds"
                    ),
                );
            }
        }
    }
}
