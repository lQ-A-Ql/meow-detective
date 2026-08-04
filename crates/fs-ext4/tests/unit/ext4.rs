use crate::format::*;
pub(crate) use crate::*;
use evidence_core::filesystem::FileSystemReader;
use evidence_core::EvidenceReader;
use std::io::{self, SeekFrom};

// ===========================================================================
// Tests
// ===========================================================================

mod cases {
    use super::*;
    use evidence_core::ReaderInfo;
    use std::io::{Read, Seek};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use testing::builders::ext4::minimal_ext4_image;

    // -----------------------------------------------------------------------
    // Fake evidence reader for in-memory fixtures
    // -----------------------------------------------------------------------

    struct FakeReader {
        data: Vec<u8>,
        pos: u64,
        info: ReaderInfo,
    }

    impl FakeReader {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                pos: 0,
                info: ReaderInfo {
                    path: std::path::PathBuf::from("fake-ext4"),
                    size: 0,
                    kind: "fake-ext4".to_string(),
                },
            }
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
            &self.info
        }
    }

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

    impl Seek for CountingReader {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.inner.seek(pos)
        }
    }

    impl EvidenceReader for CountingReader {
        fn info(&self) -> &ReaderInfo {
            self.inner.info()
        }
    }

    // -----------------------------------------------------------------------
    // Fixture builder
    // -----------------------------------------------------------------------

    fn build_ext4_fixture() -> Vec<u8> {
        minimal_ext4_image()
    }

    fn build_large_sparse_ext4_fixture(marker: &[u8]) -> (Vec<u8>, u64) {
        const LOGICAL_OFFSET: u64 = 128 * 1024 * 1024;
        let mut img = minimal_ext4_image();
        let block_size = 4096u64;
        let logical_block = (LOGICAL_OFFSET / block_size) as u32;
        let physical_block = 7u32;
        let file_size = LOGICAL_OFFSET + marker.len() as u64;

        let file_inode = &mut img[8192 + 512..8192 + 768];
        file_inode[0x04..0x08].copy_from_slice(&(file_size as u32).to_le_bytes());
        file_inode[0x6C..0x70].copy_from_slice(&((file_size >> 32) as u32).to_le_bytes());
        file_inode[0x34..0x38].copy_from_slice(&logical_block.to_le_bytes());
        file_inode[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes());
        file_inode[0x3A..0x3C].copy_from_slice(&0u16.to_le_bytes());
        file_inode[0x3C..0x40].copy_from_slice(&physical_block.to_le_bytes());

        let data_offset = physical_block as usize * block_size as usize;
        img[data_offset..data_offset + marker.len()].copy_from_slice(marker);
        (img, LOGICAL_OFFSET)
    }

    // -----------------------------------------------------------------------
    // test_superblock_magic
    // -----------------------------------------------------------------------

    #[test]
    fn test_superblock_magic() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert_eq!(ext4.data_source_name(), "ext4");
    }

    // -----------------------------------------------------------------------
    // test_block_size_calculation
    // -----------------------------------------------------------------------

    #[test]
    fn test_block_size_calculation() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert_eq!(ext4.block_size, 4096);
    }

    #[test]
    fn test_64bit_group_descriptors_use_declared_entry_width() {
        let mut img = build_ext4_fixture();
        let sb = &mut img[1024..2048];
        sb[0x20..0x24].copy_from_slice(&5u32.to_le_bytes());
        sb[0x60..0x64].copy_from_slice(&EXT4_FEATURE_INCOMPAT_64BIT.to_le_bytes());
        sb[0xFE..0x100].copy_from_slice(&64u16.to_le_bytes());

        let second_descriptor = 4096 + 64;
        img[second_descriptor + 0x08..second_descriptor + 0x0C]
            .copy_from_slice(&7u32.to_le_bytes());
        img[second_descriptor + 0x28..second_descriptor + 0x2C]
            .copy_from_slice(&1u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert_eq!(ext4.group_descriptor_size, 64);
        assert_eq!(ext4.num_block_groups, 2);

        let descriptor = ext4.read_bg_descriptor(1).unwrap();
        assert_eq!(descriptor.len(), 64);
        assert_eq!(
            inode_table_block_from_descriptor(&descriptor, true).unwrap(),
            (1u64 << 32) | 7
        );
    }

    // -----------------------------------------------------------------------
    // test_root_is_directory
    // -----------------------------------------------------------------------

    #[test]
    fn test_root_is_directory() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let root = ext4.root().unwrap();
        assert_eq!(root.name, "\\");
        assert!(root.is_dir);
        assert_eq!(root.size, 0);
    }

    // -----------------------------------------------------------------------
    // test_inode_parsing
    // -----------------------------------------------------------------------

    #[test]
    fn test_inode_parsing() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let root_inode = ext4.read_inode(2).unwrap();
        assert_eq!(Ext4Reader::inode_mode(&root_inode) & 0x4000, 0x4000);
        assert_eq!(Ext4Reader::inode_size(&root_inode).unwrap(), 4096);

        let file_inode = ext4.read_inode(3).unwrap();
        assert_eq!(Ext4Reader::inode_mode(&file_inode) & 0x8000, 0x8000);
        assert_eq!(Ext4Reader::inode_size(&file_inode).unwrap(), 11);
    }

    // -----------------------------------------------------------------------
    // test_directory_listing
    // -----------------------------------------------------------------------

    #[test]
    fn test_directory_listing() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let children = ext4.list_children("").unwrap();
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"test.txt"));
        assert!(names.contains(&"subdir"));
        assert_eq!(children.len(), 2);

        let txt = children.iter().find(|n| n.name == "test.txt").unwrap();
        assert!(!txt.is_dir);
        assert_eq!(txt.path, "test.txt");
        assert_eq!(txt.size, 11);

        let sub = children.iter().find(|n| n.name == "subdir").unwrap();
        assert!(sub.is_dir);
        assert_eq!(sub.path, "subdir");
        assert_eq!(sub.size, 0);

        let nested = ext4.list_children("subdir").unwrap();
        let hello = nested.iter().find(|node| node.name == "hello.dat").unwrap();
        assert_eq!(hello.size, 13);
    }

    // -----------------------------------------------------------------------
    // test_invalid_magic_rejected
    // -----------------------------------------------------------------------

    #[test]
    fn test_invalid_magic_rejected() {
        let mut img = build_ext4_fixture();
        img[1024 + 0x38] = 0x00;
        img[1024 + 0x39] = 0x00;

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        match Ext4Reader::open(reader, 0) {
            Ok(_) => panic!("expected error for invalid magic"),
            Err(err) => {
                assert_eq!(err.kind(), io::ErrorKind::InvalidData);
                assert!(err.to_string().contains("magic"));
            }
        }
    }

    #[test]
    fn test_invalid_block_and_inode_geometry_is_rejected() {
        let mut oversized_block = build_ext4_fixture();
        oversized_block[1024 + 0x18..1024 + 0x1C].copy_from_slice(&7u32.to_le_bytes());
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(oversized_block));
        let error = Ext4Reader::open(reader, 0).err().unwrap();
        assert!(error.to_string().contains("log block size"));

        let mut undersized_inode = build_ext4_fixture();
        undersized_inode[1024 + 0x58..1024 + 0x5A].copy_from_slice(&64u16.to_le_bytes());
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(undersized_inode));
        let error = Ext4Reader::open(reader, 0).err().unwrap();
        assert!(error.to_string().contains("inode size"));
    }

    // -----------------------------------------------------------------------
    // test_open_and_read_file
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_and_read_file() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let mut file = ext4.open_file("test.txt").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "Hello World");
    }

    #[test]
    fn test_large_sparse_file_range_reads_only_requested_extent() {
        let marker = b"EXT4-RANGE-ONLY";
        let (img, offset) = build_large_sparse_ext4_fixture(marker);
        let bytes_read = Arc::new(AtomicUsize::new(0));
        let reader: Box<dyn EvidenceReader> =
            Box::new(CountingReader::new(img, Arc::clone(&bytes_read)));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        bytes_read.store(0, Ordering::Relaxed);
        let bytes = ext4
            .read_file_range("test.txt", offset, marker.len())
            .unwrap();

        assert_eq!(bytes, marker);
        assert!(
            bytes_read.load(Ordering::Relaxed) < 32 * 1024,
            "range path should not read the 128 MiB sparse prefix"
        );
    }

    #[test]
    fn test_unwritten_extent_range_zero_fills_without_reading_data_block() {
        let mut img = build_ext4_fixture();
        let file_inode = &mut img[8192 + 512..8192 + 768];
        file_inode[0x04..0x08].copy_from_slice(&4096u32.to_le_bytes());
        file_inode[0x38..0x3A].copy_from_slice(&(0x8000u16 | 1).to_le_bytes());
        file_inode[0x3C..0x40].copy_from_slice(&4u32.to_le_bytes());
        img[16384..16384 + 11].copy_from_slice(b"NOT-ZERO!!!");

        let bytes_read = Arc::new(AtomicUsize::new(0));
        let reader: Box<dyn EvidenceReader> =
            Box::new(CountingReader::new(img, Arc::clone(&bytes_read)));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        bytes_read.store(0, Ordering::Relaxed);
        let bytes = ext4.read_file_range("test.txt", 0, 16).unwrap();

        assert_eq!(bytes, vec![0u8; 16]);
        assert!(
            bytes_read.load(Ordering::Relaxed) < 16 * 1024,
            "unwritten data extent should not read the physical data block"
        );
    }

    // -----------------------------------------------------------------------
    // test_open_file_in_subdirectory
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_file_in_subdirectory() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let mut file = ext4.open_file("subdir/hello.dat").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "Hello subdir!");
    }

    // -----------------------------------------------------------------------
    // test_open_nonexistent_file
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_nonexistent_file() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        match ext4.open_file("nonexistent.txt") {
            Ok(_) => panic!("expected error for non-existent file"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::NotFound),
        }
    }

    // -----------------------------------------------------------------------
    // test_fast_symlink
    // -----------------------------------------------------------------------

    #[test]
    fn test_fast_symlink() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let sym_inode = ext4.read_inode(6).unwrap();
        let mode = Ext4Reader::inode_mode(&sym_inode);
        assert_eq!(mode & 0xF000, S_IFLNK, "inode 6 should be a symlink");

        let target = ext4.read_symlink_target(&sym_inode).unwrap();
        assert_eq!(target, "/usr/bin/perl");
    }

    // -----------------------------------------------------------------------
    // test_extent_tree_depth_one
    // -----------------------------------------------------------------------

    #[test]
    fn test_extent_tree_depth_one() {
        let block_size: u64 = 4096;
        let total_blocks: u64 = 10;
        let total_size = (total_blocks * block_size) as usize;
        let mut img = vec![0u8; total_size];

        // Superblock
        let sb_off = 1024usize;
        img[sb_off..sb_off + 0x04].copy_from_slice(&16u32.to_le_bytes());
        img[sb_off + 0x04..sb_off + 0x08].copy_from_slice(&(total_blocks as u32).to_le_bytes());
        img[sb_off + 0x14..sb_off + 0x18].copy_from_slice(&0u32.to_le_bytes());
        img[sb_off + 0x18..sb_off + 0x1C].copy_from_slice(&2u32.to_le_bytes());
        img[sb_off + 0x20..sb_off + 0x24].copy_from_slice(&32768u32.to_le_bytes());
        img[sb_off + 0x28..sb_off + 0x2C].copy_from_slice(&16u32.to_le_bytes());
        img[sb_off + 0x38..sb_off + 0x3A].copy_from_slice(&EXT4_MAGIC.to_le_bytes());
        img[sb_off + 0x58..sb_off + 0x5A].copy_from_slice(&256u16.to_le_bytes());

        // BG descriptor
        img[4096 + 0x08..4096 + 0x0C].copy_from_slice(&2u32.to_le_bytes());

        // Inode 2 (root): depth-1 extent tree
        let ri = &mut img[8192 + 256..8192 + 512];
        ri[0x00..0x02].copy_from_slice(&0x41EDu16.to_le_bytes()); // dir
        ri[0x04..0x08].copy_from_slice(&4096u32.to_le_bytes()); // i_size
        ri[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes()); // i_blocks
        ri[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        ri[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        ri[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        ri[0x2E..0x30].copy_from_slice(&1u16.to_le_bytes()); // eh_depth=1
                                                             // Index entry (+12): ei_block=0, ei_leaf_lo=block 5
        ri[0x38..0x3C].copy_from_slice(&5u32.to_le_bytes()); // ei_leaf_lo=block 5

        // Block 5: leaf extent -> block 3
        let leaf = &mut img[20480..20480 + 4096];
        leaf[0x00..0x02].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        leaf[0x02..0x04].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        leaf[0x04..0x06].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
                                                               // Extent at +12: ee_len=1 at +16, ee_start_lo=3 at +20
        leaf[0x10..0x12].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        leaf[0x14..0x18].copy_from_slice(&3u32.to_le_bytes()); // ee_start_lo=3

        // Block 3: root dir data with "f.txt"
        let rd = &mut img[12288..12288 + 4096];
        rd[0x00..0x04].copy_from_slice(&2u32.to_le_bytes());
        rd[0x04..0x06].copy_from_slice(&12u16.to_le_bytes());
        rd[0x06] = 1;
        rd[0x07] = 2;
        rd[0x08] = b'.';
        rd[12..16].copy_from_slice(&2u32.to_le_bytes());
        rd[16..18].copy_from_slice(&12u16.to_le_bytes());
        rd[18] = 2;
        rd[19] = 2;
        rd[20..22].copy_from_slice(b"..");
        rd[24..28].copy_from_slice(&3u32.to_le_bytes());
        rd[28..30].copy_from_slice(&24u16.to_le_bytes());
        rd[30] = 5;
        rd[31] = 1;
        rd[32..37].copy_from_slice(b"f.txt");

        // Inode 3: f.txt -> block 4
        let fi = &mut img[8192 + 512..8192 + 768];
        fi[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes());
        fi[0x04..0x08].copy_from_slice(&11u32.to_le_bytes());
        fi[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes());
        fi[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        fi[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes());
        fi[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes());
        fi[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        fi[0x3C..0x40].copy_from_slice(&4u32.to_le_bytes()); // ee_start_lo=4

        img[16384..16384 + 11].copy_from_slice(b"depth1 test");

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let children = ext4.list_children("").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "f.txt");

        let mut file = ext4.open_file("f.txt").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "depth1 test");
    }

    // -----------------------------------------------------------------------
    // test_64bit_block_number
    // -----------------------------------------------------------------------

    #[test]
    fn test_64bit_block_number() {
        // Verify Ext4Extent::parse reads ee_start_hi correctly
        let extent_bytes = [
            0x00, 0x00, 0x00, 0x00, // ee_block
            0x01, 0x00, // ee_len = 1
            0xAB, 0xCD, // ee_start_hi = 0xCDAB
            0x78, 0x56, 0x34, 0x12, // ee_start_lo = 0x12345678
        ];
        let extent = Ext4Extent::parse(&extent_bytes).unwrap();
        assert_eq!(extent.ee_len, 1);
        assert_eq!(extent.ee_start_hi, 0xCDAB);
        assert_eq!(extent.ee_start_lo, 0x12345678);

        // Verify 64-bit merge
        let start_block = ((extent.ee_start_hi as u64) << 32) | (extent.ee_start_lo as u64);
        assert_eq!(start_block, 0xCDAB_12345678u64);
    }

    // -----------------------------------------------------------------------
    // test_data_source_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_data_source_name() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert_eq!(ext4.data_source_name(), "ext4");
    }

    // -----------------------------------------------------------------------
    // test_list_nonexistent_path
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_nonexistent_path() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let err = ext4.list_children("no_such_dir").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
