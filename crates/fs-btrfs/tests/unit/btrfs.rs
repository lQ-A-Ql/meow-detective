pub(crate) use crate::*;
use evidence_core::filesystem::FileSystemReader;
use evidence_core::EvidenceReader;
use std::io;

const CHUNK_TREE_OBJECTID: u64 = 3;
const EXTENT_REGULAR: u8 = 1;
const FT_REG_FILE: u8 = 1;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

#[path = "support.rs"]
mod support;
pub(crate) use support::FakeReader;

mod cases {
    use super::*;
    use std::io::Read;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct CountingReader {
        inner: FakeReader,
        bytes_read: Arc<AtomicUsize>,
    }

    impl CountingReader {
        fn new(data: Vec<u8>, bytes_read: Arc<AtomicUsize>) -> Self {
            Self {
                inner: FakeReader::new(data),
                bytes_read,
            }
        }
    }

    impl Read for CountingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.bytes_read.fetch_add(n, Ordering::Relaxed);
            Ok(n)
        }
    }

    impl std::io::Seek for CountingReader {
        fn seek(&mut self, pos: std::io::SeekFrom) -> io::Result<u64> {
            self.inner.seek(pos)
        }
    }

    impl EvidenceReader for CountingReader {
        fn info(&self) -> &evidence_core::ReaderInfo {
            self.inner.info()
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
        let array_size: u32 = 0x51 + (4 - (0x51 % 4));
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
            d[136..144].copy_from_slice(&1_700_000_000i64.to_le_bytes());
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
            &make_inode(S_IFDIR | 0o755, 0, 3),
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
            &make_inode(S_IFREG | 0o644, file_content.len() as u64, 1),
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
            &make_inode(S_IFDIR | 0o755, 0, 2),
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
            &make_inode(S_IFREG | 0o444, nested_content.len() as u64, 1),
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

    fn build_large_sparse_btrfs_fixture(marker: &[u8]) -> (Vec<u8>, u64) {
        const LOGICAL_OFFSET: u64 = 128 * 1024 * 1024;
        let mut img = build_btrfs_fixture();
        let block = |n: u64| -> usize { (n * 4096) as usize };
        let file_size = LOGICAL_OFFSET + marker.len() as u64;

        {
            let fs = &mut img[block(19)..block(20)];
            let items = BtrfsReader::parse_leaf_items(fs, 9).unwrap();
            let inode = items
                .iter()
                .find(|item| item.key.objectid == 257 && item.key.ty == INODE_ITEM_KEY)
                .unwrap();
            let inode_start = inode.data_offset as usize;
            fs[inode_start + 16..inode_start + 24].copy_from_slice(&file_size.to_le_bytes());

            let extent_index = items
                .iter()
                .position(|item| item.key.objectid == 257 && item.key.ty == EXTENT_DATA_KEY)
                .unwrap();
            let key_offset = BTRFS_HEADER_SIZE + extent_index * LEAF_ITEM_SIZE + 9;
            fs[key_offset..key_offset + 8].copy_from_slice(&LOGICAL_OFFSET.to_le_bytes());

            let extent = &items[extent_index];
            let extent_start = extent.data_offset as usize;
            fs[extent_start + 45..extent_start + 53]
                .copy_from_slice(&(marker.len() as u64).to_le_bytes());
        }

        img[block(20)..block(20) + marker.len()].copy_from_slice(marker);
        (img, LOGICAL_OFFSET)
    }

    fn build_cross_leaf_btrfs_fixture() -> Vec<u8> {
        let mut img = build_btrfs_fixture();
        let block = |n: u64| -> usize { (n * 4096) as usize };

        {
            let fs = &mut img[block(19)..block(20)];
            fs[0x61] = 1;
            fs[0x5D..0x61].copy_from_slice(&2u32.to_le_bytes());
            let first = BTRFS_HEADER_SIZE;
            fs[first..first + 8].copy_from_slice(&256u64.to_le_bytes());
            fs[first + 8] = INODE_ITEM_KEY;
            fs[first + 9..first + 17].copy_from_slice(&0u64.to_le_bytes());
            fs[first + 17..first + 25].copy_from_slice(&0x15000u64.to_le_bytes());
            fs[first + 25..first + 33].copy_from_slice(&1u64.to_le_bytes());
            let second = first + INTERNAL_ITEM_SIZE;
            fs[second..second + 8].copy_from_slice(&257u64.to_le_bytes());
            fs[second + 8] = EXTENT_DATA_KEY;
            fs[second + 9..second + 17].copy_from_slice(&6u64.to_le_bytes());
            fs[second + 17..second + 25].copy_from_slice(&0x16000u64.to_le_bytes());
            fs[second + 25..second + 33].copy_from_slice(&1u64.to_le_bytes());
        }

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

        let first_chunk = b"hello ";
        let second_chunk = b"world";
        let mut inline_first = vec![0u8; 21 + first_chunk.len()];
        inline_first[20] = EXTENT_INLINE;
        inline_first[21..].copy_from_slice(first_chunk);
        let mut inline_second = vec![0u8; 21 + second_chunk.len()];
        inline_second[20] = EXTENT_INLINE;
        inline_second[21..].copy_from_slice(second_chunk);

        {
            let leaf = &mut img[block(21)..block(22)];
            leaf[0x30..0x38].copy_from_slice(&0x15000u64.to_le_bytes());
            leaf[0x5D..0x61].copy_from_slice(&1u32.to_le_bytes());
            leaf[0x61] = 0;
            let mut data_off = 4096usize;
            put_item(
                leaf,
                0,
                257,
                EXTENT_DATA_KEY,
                0,
                &inline_first,
                &mut data_off,
            );
        }
        {
            let leaf = &mut img[block(22)..block(23)];
            leaf[0x30..0x38].copy_from_slice(&0x16000u64.to_le_bytes());
            leaf[0x5D..0x61].copy_from_slice(&1u32.to_le_bytes());
            leaf[0x61] = 0;
            let mut data_off = 4096usize;
            put_item(
                leaf,
                0,
                257,
                EXTENT_DATA_KEY,
                first_chunk.len() as u64,
                &inline_second,
                &mut data_off,
            );
        }

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
        assert_eq!(btrfs._sectorsize, 4096);
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
        assert!(!file.read_only);
        assert_eq!(
            file.modified_at.expect("modified timestamp").timestamp(),
            1_700_000_000
        );

        let dir = sv.iter().find(|n| n.name == "subdir").unwrap();
        assert!(dir.is_dir);

        let nested = btrfs.list_children("default/subdir").unwrap();
        assert!(
            nested
                .iter()
                .find(|node| node.name == "nested.dat")
                .expect("nested file")
                .read_only
        );
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

    #[test]
    fn test_large_sparse_file_range_reads_only_requested_extent() {
        let marker = b"BTRFS-RANGE-ONLY";
        let (img, offset) = build_large_sparse_btrfs_fixture(marker);
        let bytes_read = Arc::new(AtomicUsize::new(0));
        let reader: Box<dyn EvidenceReader> =
            Box::new(CountingReader::new(img, Arc::clone(&bytes_read)));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        bytes_read.store(0, Ordering::Relaxed);
        let bytes = btrfs
            .read_file_range("default/file.txt", offset, marker.len())
            .unwrap();

        assert_eq!(bytes, marker);
        assert!(
            bytes_read.load(Ordering::Relaxed) < 64 * 1024,
            "range path should not read the 128 MiB sparse prefix"
        );
    }

    #[test]
    fn test_file_range_reads_extent_items_across_multiple_leaves() {
        let img = build_cross_leaf_btrfs_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let btrfs = BtrfsReader::open(reader, 0).unwrap();

        let bytes = btrfs
            .read_file_extents_range(0x13000, 257, 11, 0, 11)
            .unwrap();

        assert_eq!(bytes, b"hello world");
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
        assert!(btrfs._sectorsize > 0, "sectorsize must be > 0");
        assert!(btrfs.nodesize > 0, "nodesize must be > 0");
    }
}
