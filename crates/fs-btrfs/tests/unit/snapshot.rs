use super::*;
use crate::{
    tests::FakeReader, BTRFS_HEADER_SIZE, BTRFS_MAGIC, CHUNK_ITEM_KEY, DIR_INDEX_KEY,
    EXTENT_DATA_KEY, EXTENT_INLINE, FIRST_FREE_OBJECTID, INODE_ITEM_KEY, LEAF_ITEM_SIZE,
    ROOT_BACKREF_KEY, ROOT_ITEM_KEY,
};
use evidence_core::{filesystem::FileSystemReader, EvidenceReader};

const CHUNK_TREE_OBJECTID: u64 = 3;
const EXTENT_REGULAR: u8 = 1;
const FT_REG_FILE: u8 = 1;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

// -------------------------------------------------------------------
// Btrfs fixture with a snapshot
// -------------------------------------------------------------------
//
// Layout (nodesize = 4096, 30 blocks = 0x1E000 bytes):
//
//  Block  Offset    Logical   Content
//  -----  --------  --------  --------------------------------
//   0-15  0x00000   --        Reserved (first 64K)
//   16    0x10000   0x10000   Superblock
//   17    0x11000   0x11000   Root tree internal node
//   18    0x12000   0x12000   Root tree leaf: ROOT_ITEM(5),
//                             ROOT_BACKREF(5,"default"),
//                             ROOT_ITEM(6),
//                             ROOT_BACKREF(6,"snapshot1")
//   19    0x13000   0x13000   FS tree leaf (default): files
//   20    0x14000   0x14000   File data "Hello from Btrfs!"
//   21    0x15000   0x15000   Snapshot1 tree leaf: files
//   22    0x16000   0x16000   Snapshot1 file data

fn build_snapshot_fixture() -> Vec<u8> {
    let nodesize: u64 = 4096;
    let total_blocks: u64 = 30;
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
    let array_size: u32 = 0x51 + (4 - (0x51 % 4));
    sb[0xC8..0xCC].copy_from_slice(&array_size.to_le_bytes());

    // ---- Root tree internal node at block 17 (0x11000) ----
    let rt = &mut img[block(17)..block(18)];
    rt[0x30..0x38].copy_from_slice(&0x11000u64.to_le_bytes());
    rt[0x5D..0x61].copy_from_slice(&2u32.to_le_bytes()); // 2 internal pointers
    rt[0x61] = 1;
    let io = BTRFS_HEADER_SIZE;
    // Key-pointer 0: FS_TREE (5) → block 18
    rt[io..io + 8].copy_from_slice(&FS_TREE_OBJECTID.to_le_bytes());
    rt[io + 8] = ROOT_ITEM_KEY;
    rt[io + 9..io + 17].copy_from_slice(&0u64.to_le_bytes());
    rt[io + 17..io + 25].copy_from_slice(&0x12000u64.to_le_bytes());
    rt[io + 25..io + 33].copy_from_slice(&1u64.to_le_bytes());
    // Key-pointer 1: snapshot1 (6) → block 18 (same leaf)
    let io1 = io + 33;
    rt[io1..io1 + 8].copy_from_slice(&6u64.to_le_bytes());
    rt[io1 + 8] = ROOT_ITEM_KEY;
    rt[io1 + 9..io1 + 17].copy_from_slice(&0u64.to_le_bytes());
    rt[io1 + 17..io1 + 25].copy_from_slice(&0x12000u64.to_le_bytes());
    rt[io1 + 25..io1 + 33].copy_from_slice(&1u64.to_le_bytes());

    // ---- Root tree leaf at block 18 (0x12000) ----
    let rtl = &mut img[block(18)..block(19)];
    rtl[0x30..0x38].copy_from_slice(&0x12000u64.to_le_bytes());
    rtl[0x61] = 0;

    let data_end = nodesize as usize;
    let mut doff = data_end;

    // Helper: write one leaf item.
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

    fn make_root_item(bytenr: u64, root_dirid: u64) -> Vec<u8> {
        let mut d = vec![0u8; 244];
        d[0..8].copy_from_slice(&1u64.to_le_bytes());
        d[40..44].copy_from_slice(&1u32.to_le_bytes());
        d[52..56].copy_from_slice(&S_IFDIR.to_le_bytes());
        d[160..168].copy_from_slice(&1u64.to_le_bytes());
        d[168..176].copy_from_slice(&root_dirid.to_le_bytes());
        d[176..184].copy_from_slice(&bytenr.to_le_bytes());
        d[184..192].copy_from_slice(&0u64.to_le_bytes());
        d[192..200].copy_from_slice(&4096u64.to_le_bytes());
        d[216..220].copy_from_slice(&1u32.to_le_bytes());
        d
    }

    fn make_root_backref(name: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; 18 + name.len()];
        d[0..8].copy_from_slice(&FIRST_FREE_OBJECTID.to_le_bytes());
        d[8..16].copy_from_slice(&0u64.to_le_bytes());
        d[16..18].copy_from_slice(&(name.len() as u16).to_le_bytes());
        d[18..18 + name.len()].copy_from_slice(name);
        d
    }

    // Item 0: ROOT_ITEM (5,132,0) — default subvol, tree at block 19
    put_item(
        rtl,
        0,
        FS_TREE_OBJECTID,
        ROOT_ITEM_KEY,
        0,
        &make_root_item(0x13000, FIRST_FREE_OBJECTID),
        &mut doff,
    );
    // Item 1: ROOT_BACKREF (5,144,0) — name "default"
    put_item(
        rtl,
        1,
        FS_TREE_OBJECTID,
        ROOT_BACKREF_KEY,
        0,
        &make_root_backref(b"default"),
        &mut doff,
    );
    // Item 2: ROOT_ITEM (6,132,0) — snapshot1, tree at block 21
    put_item(
        rtl,
        2,
        6,
        ROOT_ITEM_KEY,
        0,
        &make_root_item(0x15000, FIRST_FREE_OBJECTID),
        &mut doff,
    );
    // Item 3: ROOT_BACKREF (6,144,0) — name "snapshot1"
    put_item(
        rtl,
        3,
        6,
        ROOT_BACKREF_KEY,
        0,
        &make_root_backref(b"snapshot1"),
        &mut doff,
    );
    rtl[0x5D..0x61].copy_from_slice(&4u32.to_le_bytes());

    // ---- Default subvol FS tree leaf at block 19 (0x13000) ----
    let fs = &mut img[block(19)..block(20)];
    fs[0x30..0x38].copy_from_slice(&0x13000u64.to_le_bytes());
    fs[0x61] = 0;
    let fs_data_end = nodesize as usize;
    let mut fs_doff = fs_data_end;

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

    let file_content = b"Hello from Btrfs!";
    let file_len = file_content.len() as u64;

    // Default subvol files: root dir (256), file.txt (257)
    put_item(
        fs,
        0,
        256,
        INODE_ITEM_KEY,
        0,
        &make_inode(S_IFDIR, 0, 2),
        &mut fs_doff,
    );
    put_item(
        fs,
        1,
        256,
        DIR_INDEX_KEY,
        1,
        &make_dir_entry(b"file.txt", 257, FT_REG_FILE),
        &mut fs_doff,
    );
    put_item(
        fs,
        2,
        257,
        INODE_ITEM_KEY,
        0,
        &make_inode(S_IFREG, file_len, 1),
        &mut fs_doff,
    );
    put_item(
        fs,
        3,
        257,
        EXTENT_DATA_KEY,
        0,
        &make_regular_extent(0x14000, file_len, file_len),
        &mut fs_doff,
    );
    fs[0x5D..0x61].copy_from_slice(&4u32.to_le_bytes());

    // ---- Block 20: default subvol file data ----
    img[block(20)..block(20) + file_content.len()].copy_from_slice(file_content);

    // ---- Snapshot1 FS tree leaf at block 21 (0x15000) ----
    let snap = &mut img[block(21)..block(22)];
    snap[0x30..0x38].copy_from_slice(&0x15000u64.to_le_bytes());
    snap[0x61] = 0;
    let snap_data_end = nodesize as usize;
    let mut snap_doff = snap_data_end;

    let snap_content = b"Snapshot file data!!";
    let snap_len = snap_content.len() as u64;

    let extra_content = b"Another file in snapshot";
    let extra_len = extra_content.len() as u64;

    fn make_inline_extent(content: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; 21 + content.len()];
        d[0..8].copy_from_slice(&1u64.to_le_bytes());
        d[8..16].copy_from_slice(&(content.len() as u64).to_le_bytes());
        d[20] = EXTENT_INLINE;
        d[21..21 + content.len()].copy_from_slice(content);
        d
    }

    // Snapshot files: root dir(256), snap_file.txt(257), extra.dat(258)
    put_item(
        snap,
        0,
        256,
        INODE_ITEM_KEY,
        0,
        &make_inode(S_IFDIR, 0, 3),
        &mut snap_doff,
    );
    put_item(
        snap,
        1,
        256,
        DIR_INDEX_KEY,
        1,
        &make_dir_entry(b"snap_file.txt", 257, FT_REG_FILE),
        &mut snap_doff,
    );
    put_item(
        snap,
        2,
        257,
        INODE_ITEM_KEY,
        0,
        &make_inode(S_IFREG, snap_len, 1),
        &mut snap_doff,
    );
    put_item(
        snap,
        3,
        257,
        EXTENT_DATA_KEY,
        0,
        &make_inline_extent(snap_content),
        &mut snap_doff,
    );
    put_item(
        snap,
        4,
        256,
        DIR_INDEX_KEY,
        2,
        &make_dir_entry(b"extra.dat", 258, FT_REG_FILE),
        &mut snap_doff,
    );
    put_item(
        snap,
        5,
        258,
        INODE_ITEM_KEY,
        0,
        &make_inode(S_IFREG, extra_len, 1),
        &mut snap_doff,
    );
    put_item(
        snap,
        6,
        258,
        EXTENT_DATA_KEY,
        0,
        &make_inline_extent(extra_content),
        &mut snap_doff,
    );
    snap[0x5D..0x61].copy_from_slice(&7u32.to_le_bytes());

    img
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[test]
fn test_list_snapshots() {
    let img = build_snapshot_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let btrfs = BtrfsReader::open(reader, 0).unwrap();

    let snapshots = btrfs.list_snapshots().unwrap();
    assert!(!snapshots.is_empty(), "should find at least one snapshot");

    let snap = snapshots.iter().find(|s| s.name == "snapshot1");
    assert!(snap.is_some(), "should find snapshot named 'snapshot1'");

    let snap = snap.unwrap();
    assert_eq!(snap.id, 6);
    assert_eq!(snap.root_dirid, FIRST_FREE_OBJECTID);
    assert!(snap.tree_root_bytenr > 0);

    // FS_TREE should NOT appear as a snapshot.
    assert!(
        !snapshots.iter().any(|s| s.id == FS_TREE_OBJECTID),
        "default subvolume should not be listed as snapshot"
    );
}

#[test]
fn test_read_snapshot() {
    let img = build_snapshot_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let btrfs = BtrfsReader::open(reader, 0).unwrap();

    let files = btrfs.read_snapshot(6).unwrap();
    assert!(!files.is_empty(), "snapshot1 should have files");

    let paths: Vec<&str> = files.iter().map(|n| n.path.as_str()).collect();
    assert!(
        paths.contains(&"snap_file.txt"),
        "snapshot1 should contain snap_file.txt, got {:?}",
        paths
    );
    assert!(
        paths.contains(&"extra.dat"),
        "snapshot1 should contain extra.dat, got {:?}",
        paths
    );

    // Verify file content is readable through the normal BtrfsReader path.
    let mut f = btrfs.open_file("snapshot1/snap_file.txt").unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut f, &mut s).unwrap();
    assert_eq!(s, "Snapshot file data!!");
}

#[test]
fn test_diff_snapshots() {
    let img = build_snapshot_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let btrfs = BtrfsReader::open(reader, 0).unwrap();

    // Diff default(5) vs snapshot1(6)
    let diff = btrfs.diff_snapshots(5, 6).unwrap();

    // default has "file.txt"; snapshot1 does not → removed
    let removed_paths: Vec<&str> = diff.removed.iter().map(|n| n.path.as_str()).collect();
    assert!(
        removed_paths.contains(&"file.txt"),
        "diff should show file.txt as removed, got {:?}",
        removed_paths
    );

    // snapshot1 has "snap_file.txt" and "extra.dat"; default does not → added
    let added_paths: Vec<&str> = diff.added.iter().map(|n| n.path.as_str()).collect();
    assert!(
        added_paths.contains(&"snap_file.txt"),
        "diff should show snap_file.txt as added, got {:?}",
        added_paths
    );
    assert!(
        added_paths.contains(&"extra.dat"),
        "diff should show extra.dat as added, got {:?}",
        added_paths
    );

    // No files exist in both with different sizes.
    assert!(
        diff.changed.is_empty(),
        "no files should be changed between fresh snapshots, got {:?}",
        diff.changed
    );
}

#[test]
fn test_diff_same_snapshot() {
    let img = build_snapshot_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let btrfs = BtrfsReader::open(reader, 0).unwrap();

    // Diff snapshot1 vs itself should be empty.
    let diff = btrfs.diff_snapshots(6, 6).unwrap();
    assert!(diff.added.is_empty());
    assert!(diff.removed.is_empty());
    assert!(diff.changed.is_empty());
}

#[test]
fn test_snapshot_nonexistent() {
    let img = build_snapshot_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let btrfs = BtrfsReader::open(reader, 0).unwrap();

    let result = btrfs.read_snapshot(999);
    assert!(result.is_err(), "nonexistent snapshot should fail");
}

#[test]
fn test_read_default_as_subvolume() {
    let img = build_snapshot_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let btrfs = BtrfsReader::open(reader, 0).unwrap();

    // Reading FS_TREE (5) as a snapshot should yield its files.
    let files = btrfs.read_snapshot(5).unwrap();
    let paths: Vec<&str> = files.iter().map(|n| n.path.as_str()).collect();
    assert!(
        paths.contains(&"file.txt"),
        "default subvol should contain file.txt, got {:?}",
        paths
    );
}

#[test]
fn test_diff_file_trees_size_change() {
    let mut f1 = fs_node("data.bin", false, 100, None, None, None);
    f1.path = "data.bin".to_string();
    let mut f2 = fs_node("data.bin", false, 200, None, None, None);
    f2.path = "data.bin".to_string();

    let diff = diff_file_trees(&[f1], &[f2]);
    assert_eq!(diff.changed.len(), 1);
    assert_eq!(diff.changed[0].path, "data.bin");
    assert_eq!(diff.changed[0].old_size, 100);
    assert_eq!(diff.changed[0].new_size, 200);
}
