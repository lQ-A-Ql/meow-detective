//! Making a new single-device filesystem (#259).
//!
//! Every other write path in this crate edits a filesystem that already
//! exists: a transaction rewrites the trees a mount found. This one builds
//! the trees from nothing: three chunks, the nine trees a fresh filesystem
//! has, and the superblock and its mirrors that point at them.
//!
//! # Where the layout comes from
//!
//! **Measurement.** The standard formatter (btrfs-progs 6.2) was run on
//! devices from 300 MiB to 300 GiB, and every item it left was read back
//! raw. What is written here is that layout:
//!
//! ```text
//! device offset    logical       what
//! 0 .. 1 MiB                     reserved; the superblock at 64 KiB
//! 13 MiB           13 MiB        DATA, single, 8 MiB
//! 21 MiB, 29 MiB   21 MiB        SYSTEM, DUP, 8 MiB: the chunk tree
//! 37 MiB, 37+M     29 MiB        METADATA, DUP, M: every other tree
//! ```
//!
//! The chunk tree is the first block of the system chunk; the extent,
//! filesystem, checksum, data-relocation, UUID, device, free-space and
//! root trees are the first eight blocks of the metadata chunk, one leaf
//! each. The items in them are the ones the standard formatter writes, in
//! its encoding: the device item and three chunk items; a block group
//! item per chunk and a skinny metadata item per tree block; a device
//! extent per stripe; the root directory of the default subvolume and of
//! the data-relocation tree; the UUID of the default subvolume; and a
//! free-space record per chunk.
//!
//! Two things are the formatter's own process and not copied: it builds
//! the filesystem in temporary chunks at 1 MiB and 5 MiB and moves it,
//! leaving free-space records for chunks that no longer exist, and it
//! commits six times. This writes the final state once, at generation 1.
//!
//! # What it makes
//!
//! The standard formatter's defaults for one device: metadata and system
//! DUP, data single, the free-space tree, mixed back-references, extended
//! inode references, skinny metadata items and no-holes. Checksum
//! crc32c, xxhash64, sha256 or blake2b; node sizes from 4 KiB to 64 KiB;
//! 4 KiB sectors.

use crate::chunk::DiskKey;
use crate::error::{Error, Result};
use crate::superblock::{ChecksumType, Superblock};
use crate::tree_write::{build_leaf, flags_for_new_block, BlockIdentity, LeafItem};
use fs_core::{BlockDevice, BlockRead};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;

/// The superblock's size, and where the primary and its mirrors live.
const SUPER_SIZE: usize = 4096;
const SUPER_OFFSETS: [u64; 3] = [64 * KIB, 64 * MIB, 256 * GIB];
const MAGIC: &[u8; 8] = b"_BHRfS_M";

/// The default node size, and the bounds a node size may take.
pub const DEFAULT_NODESIZE: u32 = 16384;
pub const MIN_NODESIZE: u32 = 4096;
pub const MAX_NODESIZE: u32 = 65536;
/// The only sector size every kernel mounts on a 4 KiB page.
const SECTORSIZE: u32 = 4096;
const STRIPE_LEN: u64 = 64 * KIB;
/// The label is 256 bytes and holds a terminator.
pub const MAX_LABEL_BYTES: usize = 255;
/// The smallest device this formatter makes a filesystem on.
pub const MIN_DEVICE_BYTES: u64 = 128 * MIB;

/// Where the three chunks sit: the standard formatter's places.
const DATA_START: u64 = 13 * MIB;
const DATA_LEN: u64 = 8 * MIB;
const SYS_LOGICAL: u64 = 21 * MIB;
const SYS_PHYS: [u64; 2] = [21 * MIB, 29 * MIB];
const SYS_LEN: u64 = 8 * MIB;
const META_LOGICAL: u64 = 29 * MIB;
const META_PHYS0: u64 = 37 * MIB;

/// The generation everything is written at.
const GENERATION: u64 = 1;

mod key {
    pub const INODE_ITEM: u8 = 1;
    pub const INODE_REF: u8 = 12;
    pub const DIR_ITEM: u8 = 84;
    pub const ROOT_ITEM: u8 = 132;
    pub const METADATA_ITEM: u8 = 169;
    pub const TREE_BLOCK_REF: u8 = 176;
    pub const BLOCK_GROUP_ITEM: u8 = 192;
    pub const FREE_SPACE_INFO: u8 = 198;
    pub const FREE_SPACE_EXTENT: u8 = 199;
    pub const DEV_EXTENT: u8 = 204;
    pub const DEV_ITEM: u8 = 216;
    pub const CHUNK_ITEM: u8 = 228;
    pub const UUID_KEY_SUBVOL: u8 = 251;
}

mod oid {
    pub const ROOT_TREE: u64 = 1;
    pub const EXTENT_TREE: u64 = 2;
    pub const CHUNK_TREE: u64 = 3;
    pub const DEV_TREE: u64 = 4;
    pub const FS_TREE: u64 = 5;
    pub const ROOT_TREE_DIR: u64 = 6;
    pub const CSUM_TREE: u64 = 7;
    pub const UUID_TREE: u64 = 9;
    pub const FREE_SPACE_TREE: u64 = 10;
    pub const FIRST_FREE: u64 = 256;
    pub const DATA_RELOC_TREE: u64 = u64::MAX - 8;
    pub const DEV_ITEMS: u64 = 1;
    pub const FIRST_CHUNK_TREE: u64 = 256;
}

mod bg {
    pub const DATA: u64 = 1 << 0;
    pub const SYSTEM: u64 = 1 << 1;
    pub const METADATA: u64 = 1 << 2;
    pub const DUP: u64 = 1 << 5;
}

/// `compat_ro`: the free-space tree, and that it is valid.
const COMPAT_RO: u64 = 0x3;
/// `incompat`: mixed back-references, extended inode refs, skinny
/// metadata, no-holes.
const INCOMPAT: u64 = 0x1 | 0x40 | 0x100 | 0x200;

/// What the caller can choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Tree block size in bytes.
    pub nodesize: u32,
    /// The checksum every block carries.
    pub csum: ChecksumType,
    /// Volume label.
    pub label: Option<String>,
    /// Filesystem UUID, or `None` for a random one.
    pub uuid: Option<[u8; 16]>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            nodesize: DEFAULT_NODESIZE,
            csum: ChecksumType::Crc32c,
            label: None,
            uuid: None,
        }
    }
}

/// The geometry, worked out before anything is written.
#[derive(Debug, Clone)]
pub struct Plan {
    total_bytes: u64,
    nodesize: u32,
    csum: ChecksumType,
    label: String,
    fsid: [u8; 16],
    chunk_tree_uuid: [u8; 16],
    dev_uuid: [u8; 16],
    subvol_uuid: [u8; 16],
    meta_len: u64,
}

impl Plan {
    pub fn nodesize(&self) -> u32 {
        self.nodesize
    }
    pub fn metadata_chunk_bytes(&self) -> u64 {
        self.meta_len
    }
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    fn n(&self) -> u64 {
        u64::from(self.nodesize)
    }
    /// The trees in the metadata chunk, in the order their blocks are laid
    /// out from its start.
    fn meta_block(&self, tree: u64) -> u64 {
        let index = match tree {
            oid::EXTENT_TREE => 0,
            oid::FS_TREE => 1,
            oid::CSUM_TREE => 2,
            oid::DATA_RELOC_TREE => 3,
            oid::UUID_TREE => 4,
            oid::DEV_TREE => 5,
            oid::FREE_SPACE_TREE => 6,
            oid::ROOT_TREE => 7,
            other => unreachable!("tree {other} is not in the metadata chunk"),
        };
        META_LOGICAL + index * self.n()
    }
    fn chunk_root(&self) -> u64 {
        SYS_LOGICAL
    }
    fn meta_phys(&self) -> [u64; 2] {
        [META_PHYS0, META_PHYS0 + self.meta_len]
    }
    fn bytes_used(&self) -> u64 {
        9 * self.n()
    }
    fn dev_bytes_used(&self) -> u64 {
        DATA_LEN + 2 * SYS_LEN + 2 * self.meta_len
    }
}

/// Work out the geometry for a device of `device_bytes`.
///
/// # Errors
///
/// [`Error::InvalidGeometry`] for a device too small, a node size out of
/// range or a label too long.
pub fn plan(device_bytes: u64, opts: &Options) -> Result<Plan> {
    let n = opts.nodesize;
    if !(MIN_NODESIZE..=MAX_NODESIZE).contains(&n) || !n.is_power_of_two() {
        return Err(Error::InvalidGeometry(format!(
            "node size {n} is not a power of two from {MIN_NODESIZE} to {MAX_NODESIZE}"
        )));
    }
    let label = opts.label.clone().unwrap_or_default();
    if label.len() > MAX_LABEL_BYTES {
        return Err(Error::InvalidGeometry(format!(
            "label {label:?} is {} bytes; a Btrfs label holds {MAX_LABEL_BYTES}",
            label.len()
        )));
    }
    if device_bytes < MIN_DEVICE_BYTES {
        return Err(Error::InvalidGeometry(format!(
            "the device is too small: {device_bytes} bytes, and this formatter needs at \
             least {MIN_DEVICE_BYTES} ({} MiB) for its system, metadata and data chunks",
            MIN_DEVICE_BYTES / MIB
        )));
    }
    // The device is used in whole sectors.
    let total_bytes = device_bytes / u64::from(SECTORSIZE) * u64::from(SECTORSIZE);
    // The metadata chunk: a gigabyte on a large device, as the standard
    // formatter does above 50 GiB; otherwise 5% of the device between
    // 32 MiB and 256 MiB, in stripe-length units.
    let meta_len = if total_bytes >= 50 * GIB {
        GIB
    } else {
        (total_bytes / 20 / STRIPE_LEN * STRIPE_LEN).clamp(32 * MIB, 256 * MIB)
    };
    if META_PHYS0 + 2 * meta_len > total_bytes {
        return Err(Error::InvalidGeometry(format!(
            "the device is too small: two copies of a {meta_len}-byte metadata chunk do \
             not fit after the system chunk"
        )));
    }
    Ok(Plan {
        total_bytes,
        nodesize: n,
        csum: opts.csum,
        label,
        fsid: opts.uuid.unwrap_or_else(random_uuid),
        chunk_tree_uuid: random_uuid(),
        dev_uuid: random_uuid(),
        subvol_uuid: random_uuid(),
        meta_len,
    })
}

/// Plan and write in one call.
pub fn format(dev: &dyn BlockDevice, opts: &Options) -> Result<()> {
    let plan = plan(dev.size_bytes(), opts)?;
    write(dev, &plan)
}

/// What `dev` already holds that a format would destroy, if it is
/// something recognisable.
pub fn existing_signature(dev: &dyn BlockRead) -> Option<&'static str> {
    let mut head = vec![0u8; 128 * 1024];
    let n = (dev.size_bytes() as usize).min(head.len());
    dev.read_at(0, &mut head[..n]).ok()?;
    let head = &head[..n];
    let at = |off: usize, magic: &[u8]| head.get(off..off + magic.len()) == Some(magic);
    if at(65536 + 64, MAGIC) {
        Some("a Btrfs filesystem")
    } else if at(0, b"XFSB") {
        Some("an XFS filesystem")
    } else if at(1080, &[0x53, 0xef]) {
        Some("an ext2/3/4 filesystem")
    } else if at(3, b"NTFS    ") {
        Some("an NTFS filesystem")
    } else if at(1024, &[0xe2, 0xe1, 0xf5, 0xe0]) {
        Some("an EROFS filesystem")
    } else if at(0, b"hsqs") {
        Some("a SquashFS filesystem")
    } else if at(3, b"EXFAT   ") {
        Some("an exFAT filesystem")
    } else if at(82, b"FAT32   ") || at(54, b"FAT1") {
        Some("a FAT filesystem")
    } else if at(512, b"EFI PART") {
        Some("a GPT partition table")
    } else if at(510, &[0x55, 0xaa]) && head[446..510].iter().any(|b| *b != 0) {
        Some("an MBR partition table")
    } else {
        None
    }
}

fn le16(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}
fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}
fn le64(v: u64) -> [u8; 8] {
    v.to_le_bytes()
}

fn k(objectid: u64, key_type: u8, offset: u64) -> DiskKey {
    DiskKey {
        objectid,
        key_type,
        offset,
    }
}

fn disk_key(key: &DiskKey) -> Vec<u8> {
    let mut out = Vec::with_capacity(17);
    out.extend_from_slice(&le64(key.objectid));
    out.push(key.key_type);
    out.extend_from_slice(&le64(key.offset));
    out
}

/// A `btrfs_timespec`: seconds and nanoseconds.
fn timespec(secs: u64) -> Vec<u8> {
    let mut out = le64(secs).to_vec();
    out.extend_from_slice(&le32(0));
    out
}

/// A 160-byte inode item.
fn inode_item(
    generation: u64,
    size: u64,
    nbytes: u64,
    nlink: u32,
    mode: u32,
    times: [u64; 4],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(160);
    out.extend_from_slice(&le64(generation));
    out.extend_from_slice(&le64(0)); // transid
    out.extend_from_slice(&le64(size));
    out.extend_from_slice(&le64(nbytes));
    out.extend_from_slice(&le64(0)); // block_group
    out.extend_from_slice(&le32(nlink));
    out.extend_from_slice(&le32(0)); // uid
    out.extend_from_slice(&le32(0)); // gid
    out.extend_from_slice(&le32(mode));
    out.extend_from_slice(&le64(0)); // rdev
    out.extend_from_slice(&le64(0)); // flags
    out.extend_from_slice(&le64(0)); // sequence
    out.extend_from_slice(&[0u8; 32]); // reserved
    for t in times {
        out.extend_from_slice(&timespec(t));
    }
    debug_assert_eq!(out.len(), 160);
    out
}

/// An inode ref: index, then the name.
fn inode_ref(index: u64, name: &[u8]) -> Vec<u8> {
    let mut out = le64(index).to_vec();
    out.extend_from_slice(&le16(name.len() as u16));
    out.extend_from_slice(name);
    out
}

/// A 439-byte root item.
fn root_item(
    plan: &Plan,
    bytenr: u64,
    root_dirid: u64,
    with_inode: bool,
    uuid: [u8; 16],
    times: u64,
) -> Vec<u8> {
    let mut out = if with_inode {
        // What the standard formatter embeds: a directory, generation 1,
        // size 3, one link, a node's worth of bytes.
        inode_item(1, 3, plan.n(), 1, 0o40755, [0; 4])
    } else {
        vec![0u8; 160]
    };
    out.extend_from_slice(&le64(GENERATION));
    out.extend_from_slice(&le64(root_dirid));
    out.extend_from_slice(&le64(bytenr));
    out.extend_from_slice(&le64(0)); // byte_limit
    out.extend_from_slice(&le64(plan.n())); // bytes_used
    out.extend_from_slice(&le64(0)); // last_snapshot
    out.extend_from_slice(&le64(0)); // flags
    out.extend_from_slice(&le32(1)); // refs
    out.extend_from_slice(&[0u8; 17]); // drop_progress
    out.push(0); // drop_level
    out.push(0); // level
    out.extend_from_slice(&le64(GENERATION)); // generation_v2
    out.extend_from_slice(&uuid);
    out.extend_from_slice(&[0u8; 16]); // parent_uuid
    out.extend_from_slice(&[0u8; 16]); // received_uuid
    out.extend_from_slice(&[0u8; 32]); // ctransid, otransid, stransid, rtransid
    out.extend_from_slice(&timespec(times)); // ctime
    out.extend_from_slice(&timespec(times)); // otime
    out.extend_from_slice(&timespec(0)); // stime
    out.extend_from_slice(&timespec(0)); // rtime
    out.extend_from_slice(&[0u8; 64]); // reserved
    debug_assert_eq!(out.len(), 439);
    out
}

/// A chunk item with its stripes.
fn chunk_item(plan: &Plan, length: u64, kind: u64, stripes: &[u64]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&le64(length));
    out.extend_from_slice(&le64(oid::EXTENT_TREE)); // owner
    out.extend_from_slice(&le64(STRIPE_LEN));
    out.extend_from_slice(&le64(kind));
    out.extend_from_slice(&le32(STRIPE_LEN as u32)); // io_align
    out.extend_from_slice(&le32(STRIPE_LEN as u32)); // io_width
    out.extend_from_slice(&le32(SECTORSIZE));
    out.extend_from_slice(&le16(stripes.len() as u16));
    out.extend_from_slice(&le16(1)); // sub_stripes
    for &phys in stripes {
        out.extend_from_slice(&le64(1)); // devid
        out.extend_from_slice(&le64(phys));
        out.extend_from_slice(&plan.dev_uuid);
    }
    out
}

/// The device item, in the chunk tree and in the superblock.
fn dev_item(plan: &Plan) -> Vec<u8> {
    let mut out = Vec::with_capacity(98);
    out.extend_from_slice(&le64(1)); // devid
    out.extend_from_slice(&le64(plan.total_bytes));
    out.extend_from_slice(&le64(plan.dev_bytes_used()));
    out.extend_from_slice(&le32(SECTORSIZE)); // io_align
    out.extend_from_slice(&le32(SECTORSIZE)); // io_width
    out.extend_from_slice(&le32(SECTORSIZE)); // sector_size
    out.extend_from_slice(&le64(0)); // type
    out.extend_from_slice(&le64(0)); // generation
    out.extend_from_slice(&le64(0)); // start_offset
    out.extend_from_slice(&le32(0)); // dev_group
    out.push(0); // seek_speed
    out.push(0); // bandwidth
    out.extend_from_slice(&plan.dev_uuid);
    out.extend_from_slice(&plan.fsid);
    debug_assert_eq!(out.len(), 98);
    out
}

fn block_group_item(used: u64, flags: u64) -> Vec<u8> {
    let mut out = le64(used).to_vec();
    out.extend_from_slice(&le64(oid::FIRST_CHUNK_TREE));
    out.extend_from_slice(&le64(flags));
    out
}

/// A skinny metadata item for a tree block owned by `root`.
fn metadata_item(root: u64) -> Vec<u8> {
    let mut out = le64(1).to_vec(); // refs
    out.extend_from_slice(&le64(GENERATION));
    out.extend_from_slice(&le64(2)); // flags: TREE_BLOCK
    out.push(key::TREE_BLOCK_REF);
    out.extend_from_slice(&le64(root));
    out
}

fn dev_extent(plan: &Plan, chunk_offset: u64, length: u64) -> Vec<u8> {
    let mut out = le64(oid::CHUNK_TREE).to_vec();
    out.extend_from_slice(&le64(oid::FIRST_CHUNK_TREE));
    out.extend_from_slice(&le64(chunk_offset));
    out.extend_from_slice(&le64(length));
    out.extend_from_slice(&plan.chunk_tree_uuid);
    out
}

/// One leaf's items, sorted, as `build_leaf` takes them.
struct Leaf(Vec<(DiskKey, Vec<u8>)>);

impl Leaf {
    fn new() -> Self {
        Leaf(Vec::new())
    }
    fn add(&mut self, key: DiskKey, data: Vec<u8>) {
        self.0.push((key, data));
    }
    fn build(mut self, sb: &Superblock, plan: &Plan, bytenr: u64, owner: u64) -> Result<Vec<u8>> {
        self.0
            .sort_by(|a, b| crate::btree::compare_keys(&a.0, &b.0));
        let items: Vec<LeafItem> = self
            .0
            .iter()
            .map(|(key, data)| LeafItem {
                key: *key,
                data: data.as_slice(),
            })
            .collect();
        build_leaf(
            sb,
            BlockIdentity {
                bytenr,
                owner,
                generation: GENERATION,
                level: 0,
                flags: flags_for_new_block(),
                chunk_tree_uuid: plan.chunk_tree_uuid,
            },
            &items,
        )
    }
}

/// The superblock, at `bytenr`.
fn superblock(plan: &Plan, bytenr: u64) -> Vec<u8> {
    use crate::superblock::offsets as o;
    let mut raw = vec![0u8; SUPER_SIZE];
    let mut put = |at: usize, bytes: &[u8]| raw[at..at + bytes.len()].copy_from_slice(bytes);
    put(o::FSID, &plan.fsid);
    put(o::BYTENR, &le64(bytenr));
    put(o::FLAGS, &le64(1)); // WRITTEN
    put(o::MAGIC, MAGIC);
    put(o::GENERATION, &le64(GENERATION));
    put(o::ROOT, &le64(plan.meta_block(oid::ROOT_TREE)));
    put(o::CHUNK_ROOT, &le64(plan.chunk_root()));
    put(o::TOTAL_BYTES, &le64(plan.total_bytes));
    put(o::BYTES_USED, &le64(plan.bytes_used()));
    put(o::ROOT_DIR_OBJECTID, &le64(oid::ROOT_TREE_DIR));
    put(o::NUM_DEVICES, &le64(1));
    put(o::SECTORSIZE, &le32(SECTORSIZE));
    put(o::NODESIZE, &le32(plan.nodesize));
    put(o::LEAFSIZE, &le32(plan.nodesize));
    put(o::STRIPESIZE, &le32(SECTORSIZE));
    put(o::CHUNK_ROOT_GENERATION, &le64(GENERATION));
    put(o::COMPAT_RO_FLAGS, &le64(COMPAT_RO));
    put(o::INCOMPAT_FLAGS, &le64(INCOMPAT));
    put(o::CSUM_TYPE, &le16(plan.csum.to_raw()));
    put(o::DEV_ITEM, &dev_item(plan));
    put(o::LABEL, plan.label.as_bytes());
    // The system chunk, which a mount needs before it can read the chunk
    // tree that describes it.
    let mut array = disk_key(&k(oid::FIRST_CHUNK_TREE, key::CHUNK_ITEM, SYS_LOGICAL));
    array.extend_from_slice(&chunk_item(plan, SYS_LEN, bg::SYSTEM | bg::DUP, &SYS_PHYS));
    put(o::SYS_CHUNK_ARRAY_SIZE, &le32(array.len() as u32));
    put(o::SYS_CHUNK_ARRAY, &array);
    let root = |tree| crate::super_write::BackupRoot {
        bytenr: plan.meta_block(tree),
        generation: GENERATION,
        level: 0,
    };
    crate::super_write::write_backup(
        &mut raw,
        GENERATION,
        &crate::super_write::BackupRoots {
            tree: root(oid::ROOT_TREE),
            chunk: crate::super_write::BackupRoot {
                bytenr: plan.chunk_root(),
                generation: GENERATION,
                level: 0,
            },
            extent: root(oid::EXTENT_TREE),
            fs: root(oid::FS_TREE),
            dev: root(oid::DEV_TREE),
            csum: root(oid::CSUM_TREE),
            total_bytes: plan.total_bytes,
            bytes_used: plan.bytes_used(),
            num_devices: 1,
        },
    );
    crate::super_write::stamp_checksum(&mut raw, plan.csum);
    raw
}

/// Make the filesystem `plan` describes on `dev`.
///
/// The primary superblock is written last, so a format interrupted
/// part-way leaves a device that does not look like a finished Btrfs.
pub fn write(dev: &dyn BlockDevice, plan: &Plan) -> Result<()> {
    let size = dev.size_bytes();
    // Old signatures: the first MiB, and the last.
    zero(dev, 0, MIB.min(size))?;
    zero(dev, size.saturating_sub(MIB), MIB.min(size))?;
    for &at in &SUPER_OFFSETS[1..] {
        if at + SUPER_SIZE as u64 <= size {
            zero(dev, at, SUPER_SIZE as u64)?;
        }
    }

    let primary = superblock(plan, SUPER_OFFSETS[0]);
    let sb = Superblock::parse(&primary)?;
    let n = plan.n();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let meta = |tree| plan.meta_block(tree);

    // The chunk tree, in the system chunk.
    let mut chunk = Leaf::new();
    chunk.add(k(oid::DEV_ITEMS, key::DEV_ITEM, 1), dev_item(plan));
    chunk.add(
        k(oid::FIRST_CHUNK_TREE, key::CHUNK_ITEM, DATA_START),
        chunk_item(plan, DATA_LEN, bg::DATA, &[DATA_START]),
    );
    chunk.add(
        k(oid::FIRST_CHUNK_TREE, key::CHUNK_ITEM, SYS_LOGICAL),
        chunk_item(plan, SYS_LEN, bg::SYSTEM | bg::DUP, &SYS_PHYS),
    );
    chunk.add(
        k(oid::FIRST_CHUNK_TREE, key::CHUNK_ITEM, META_LOGICAL),
        chunk_item(
            plan,
            plan.meta_len,
            bg::METADATA | bg::DUP,
            &plan.meta_phys(),
        ),
    );

    // The extent tree: a block group per chunk, a back-reference per
    // tree block.
    let mut extent = Leaf::new();
    extent.add(
        k(DATA_START, key::BLOCK_GROUP_ITEM, DATA_LEN),
        block_group_item(0, bg::DATA),
    );
    extent.add(
        k(SYS_LOGICAL, key::BLOCK_GROUP_ITEM, SYS_LEN),
        block_group_item(n, bg::SYSTEM | bg::DUP),
    );
    extent.add(
        k(META_LOGICAL, key::BLOCK_GROUP_ITEM, plan.meta_len),
        block_group_item(8 * n, bg::METADATA | bg::DUP),
    );
    extent.add(
        k(plan.chunk_root(), key::METADATA_ITEM, 0),
        metadata_item(oid::CHUNK_TREE),
    );
    for tree in [
        oid::EXTENT_TREE,
        oid::FS_TREE,
        oid::CSUM_TREE,
        oid::DATA_RELOC_TREE,
        oid::UUID_TREE,
        oid::DEV_TREE,
        oid::FREE_SPACE_TREE,
        oid::ROOT_TREE,
    ] {
        extent.add(k(meta(tree), key::METADATA_ITEM, 0), metadata_item(tree));
    }

    // The device tree: an extent per stripe.
    let mut devt = Leaf::new();
    devt.add(
        k(1, key::DEV_EXTENT, DATA_START),
        dev_extent(plan, DATA_START, DATA_LEN),
    );
    for phys in SYS_PHYS {
        devt.add(
            k(1, key::DEV_EXTENT, phys),
            dev_extent(plan, SYS_LOGICAL, SYS_LEN),
        );
    }
    for phys in plan.meta_phys() {
        devt.add(
            k(1, key::DEV_EXTENT, phys),
            dev_extent(plan, META_LOGICAL, plan.meta_len),
        );
    }

    // The default subvolume's root directory, and the data-relocation
    // tree's.
    let mut fst = Leaf::new();
    fst.add(
        k(oid::FIRST_FREE, key::INODE_ITEM, 0),
        inode_item(GENERATION, 0, n, 1, 0o40755, [now; 4]),
    );
    fst.add(
        k(oid::FIRST_FREE, key::INODE_REF, oid::FIRST_FREE),
        inode_ref(0, b".."),
    );
    let mut reloc = Leaf::new();
    reloc.add(
        k(oid::FIRST_FREE, key::INODE_ITEM, 0),
        inode_item(GENERATION, 0, 0, 1, 0o40755, [now, now, now, 0]),
    );
    reloc.add(
        k(oid::FIRST_FREE, key::INODE_REF, oid::FIRST_FREE),
        inode_ref(0, b".."),
    );

    // The UUID tree: the default subvolume's UUID, to its id.
    let mut uuid = Leaf::new();
    uuid.add(
        k(
            u64::from_le_bytes(plan.subvol_uuid[..8].try_into().expect("8 bytes")),
            key::UUID_KEY_SUBVOL,
            u64::from_le_bytes(plan.subvol_uuid[8..].try_into().expect("8 bytes")),
        ),
        le64(oid::FS_TREE).to_vec(),
    );

    // The free-space tree: what each chunk has free.
    let mut free = Leaf::new();
    for (start, len, used) in [
        (DATA_START, DATA_LEN, 0),
        (SYS_LOGICAL, SYS_LEN, n),
        (META_LOGICAL, plan.meta_len, 8 * n),
    ] {
        let mut info = le32(1).to_vec(); // extent_count
        info.extend_from_slice(&le32(0)); // flags
        free.add(k(start, key::FREE_SPACE_INFO, len), info);
        free.add(
            k(start + used, key::FREE_SPACE_EXTENT, len - used),
            Vec::new(),
        );
    }

    // The root tree: every other tree's root, and the root directory that
    // names the default subvolume.
    let mut root = Leaf::new();
    for (tree, dirid, with_inode, subvol, times) in [
        (oid::EXTENT_TREE, 0, true, [0u8; 16], 0),
        (oid::DEV_TREE, 0, true, [0u8; 16], 0),
        (oid::FS_TREE, oid::FIRST_FREE, true, plan.subvol_uuid, now),
        (oid::CSUM_TREE, 0, true, [0u8; 16], 0),
        (oid::UUID_TREE, 0, false, [0u8; 16], 0),
        (oid::FREE_SPACE_TREE, 0, true, [0u8; 16], 0),
        (oid::DATA_RELOC_TREE, oid::FIRST_FREE, false, [0u8; 16], 0),
    ] {
        root.add(
            k(tree, key::ROOT_ITEM, 0),
            root_item(plan, meta(tree), dirid, with_inode, subvol, times),
        );
    }
    root.add(
        k(oid::FS_TREE, key::INODE_REF, oid::ROOT_TREE_DIR),
        inode_ref(0, b"default"),
    );
    root.add(
        k(oid::ROOT_TREE_DIR, key::INODE_ITEM, 0),
        inode_item(GENERATION, 0, n, 1, 0o40755, [now; 4]),
    );
    root.add(
        k(oid::ROOT_TREE_DIR, key::INODE_REF, oid::ROOT_TREE_DIR),
        inode_ref(0, b".."),
    );
    let mut dir = disk_key(&k(oid::FS_TREE, key::ROOT_ITEM, u64::MAX));
    dir.extend_from_slice(&le64(0)); // transid
    dir.extend_from_slice(&le16(0)); // data_len
    dir.extend_from_slice(&le16(7)); // name_len
    dir.push(2); // type: directory
    dir.extend_from_slice(b"default");
    root.add(
        k(
            oid::ROOT_TREE_DIR,
            key::DIR_ITEM,
            crate::dir::name_hash(b"default"),
        ),
        dir,
    );

    // Write each tree's leaf to every copy of its chunk.
    let blocks = [
        (
            chunk.build(&sb, plan, plan.chunk_root(), oid::CHUNK_TREE)?,
            plan.chunk_root(),
        ),
        (
            extent.build(&sb, plan, meta(oid::EXTENT_TREE), oid::EXTENT_TREE)?,
            meta(oid::EXTENT_TREE),
        ),
        (
            fst.build(&sb, plan, meta(oid::FS_TREE), oid::FS_TREE)?,
            meta(oid::FS_TREE),
        ),
        (
            Leaf::new().build(&sb, plan, meta(oid::CSUM_TREE), oid::CSUM_TREE)?,
            meta(oid::CSUM_TREE),
        ),
        (
            reloc.build(&sb, plan, meta(oid::DATA_RELOC_TREE), oid::DATA_RELOC_TREE)?,
            meta(oid::DATA_RELOC_TREE),
        ),
        (
            uuid.build(&sb, plan, meta(oid::UUID_TREE), oid::UUID_TREE)?,
            meta(oid::UUID_TREE),
        ),
        (
            devt.build(&sb, plan, meta(oid::DEV_TREE), oid::DEV_TREE)?,
            meta(oid::DEV_TREE),
        ),
        (
            free.build(&sb, plan, meta(oid::FREE_SPACE_TREE), oid::FREE_SPACE_TREE)?,
            meta(oid::FREE_SPACE_TREE),
        ),
        (
            root.build(&sb, plan, meta(oid::ROOT_TREE), oid::ROOT_TREE)?,
            meta(oid::ROOT_TREE),
        ),
    ];
    for (block, logical) in &blocks {
        let copies: Vec<u64> = if *logical >= META_LOGICAL {
            plan.meta_phys()
                .iter()
                .map(|p| p + (logical - META_LOGICAL))
                .collect()
        } else {
            SYS_PHYS
                .iter()
                .map(|p| p + (logical - SYS_LOGICAL))
                .collect()
        };
        for phys in copies {
            dev.write_at(phys, block)?;
        }
    }
    dev.flush()?;

    // The mirrors, then the primary.
    for &at in &SUPER_OFFSETS[1..] {
        if at + SUPER_SIZE as u64 <= size {
            dev.write_at(at, &superblock(plan, at))?;
        }
    }
    dev.write_at(SUPER_OFFSETS[0], &primary)?;
    dev.flush()?;
    Ok(())
}

fn zero(dev: &dyn BlockDevice, offset: u64, len: u64) -> Result<()> {
    let buf = vec![0u8; len.min(4 * MIB) as usize];
    let mut done = 0;
    while done < len {
        let n = (len - done).min(buf.len() as u64);
        dev.write_at(offset + done, &buf[..n as usize])?;
        done += n;
    }
    Ok(())
}

/// A random version-4 UUID, from the operating system's generator where
/// there is one.
fn random_uuid() -> [u8; 16] {
    let mut u = [0u8; 16];
    let filled = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut u))
        .is_ok();
    if !filled {
        use std::hash::{BuildHasher, Hasher};
        for half in 0..2 {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            );
            h.write_usize(half);
            u[half * 8..half * 8 + 8].copy_from_slice(&h.finish().to_be_bytes());
        }
    }
    u[6] = (u[6] & 0x0f) | 0x40;
    u[8] = (u[8] & 0x3f) | 0x80;
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_under_the_minimum_is_too_small() {
        let e = plan(MIN_DEVICE_BYTES - 1, &Options::default()).unwrap_err();
        assert!(e.to_string().contains("too small"), "{e}");
    }

    #[test]
    fn a_node_size_outside_the_range_is_refused() {
        for n in [2048u32, 3000, 131_072] {
            let opts = Options {
                nodesize: n,
                ..Options::default()
            };
            assert!(plan(GIB, &opts).is_err(), "{n}");
        }
    }

    #[test]
    fn a_label_past_255_bytes_is_refused() {
        let opts = Options {
            label: Some("x".repeat(256)),
            ..Options::default()
        };
        assert!(plan(GIB, &opts).is_err());
    }

    /// The metadata chunk: 5% between 32 MiB and 256 MiB, a gigabyte from
    /// 50 GiB, and both copies inside the device.
    #[test]
    fn the_metadata_chunk_scales_with_the_device() {
        let m = |bytes| {
            plan(bytes, &Options::default())
                .unwrap()
                .metadata_chunk_bytes()
        };
        assert_eq!(m(128 * MIB), 32 * MIB);
        assert_eq!(
            m(GIB),
            53_673_984,
            "5% of 1 GiB, as the standard formatter chose"
        );
        assert_eq!(m(16 * GIB), 256 * MIB);
        assert_eq!(m(300 * GIB), GIB);
    }
}
