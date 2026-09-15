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
        // Inflate blocks_count so the oversized i_size stays within the
        // filesystem capacity enforced by validate_declared_size.
        img[1024 + 0x04..1024 + 0x08].copy_from_slice(&40_000u32.to_le_bytes());
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
        assert_eq!(
            Ext4Reader::inode_mode(&root_inode).unwrap() & 0x4000,
            0x4000
        );
        assert_eq!(Ext4Reader::inode_size(&root_inode).unwrap(), 4096);

        let file_inode = ext4.read_inode(3).unwrap();
        assert_eq!(
            Ext4Reader::inode_mode(&file_inode).unwrap() & 0x8000,
            0x8000
        );
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
        assert_eq!(txt.unix_mode, Some(0o100644));

        let sub = children.iter().find(|n| n.name == "subdir").unwrap();
        assert!(sub.is_dir);
        assert_eq!(sub.path, "subdir");
        assert_eq!(sub.size, 0);
        assert_eq!(sub.unix_mode, Some(0o040755));

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
    fn encrypted_ext4_inode_is_marked_and_never_read_as_plaintext() {
        let mut img = build_ext4_fixture();
        img[FILE_INODE_OFFSET + I_FLAGS_OFFSET..FILE_INODE_OFFSET + I_FLAGS_OFFSET + 4]
            .copy_from_slice(&EXT4_ENCRYPT_FL.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let file = ext4
            .list_children("")
            .unwrap()
            .into_iter()
            .find(|node| node.name == "test.txt")
            .expect("encrypted test file");
        assert!(file.encrypted);

        let error = match ext4.open_file("test.txt") {
            Ok(_) => panic!("encrypted file was read"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
        let error = ext4
            .read_file_range("test.txt", 0, 4)
            .expect_err("encrypted range was read");
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn encrypted_ext4_directory_is_marked_and_rejected() {
        let mut img = build_ext4_fixture();
        img[SUBDIR_INODE_OFFSET + I_FLAGS_OFFSET..SUBDIR_INODE_OFFSET + I_FLAGS_OFFSET + 4]
            .copy_from_slice(&EXT4_ENCRYPT_FL.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let subdir = ext4
            .list_children("")
            .unwrap()
            .into_iter()
            .find(|node| node.name == "subdir")
            .expect("encrypted subdirectory");
        assert!(subdir.encrypted);

        let error = ext4
            .list_children("subdir")
            .expect_err("encrypted directory was traversed");
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn directory_listing_does_not_fabricate_nodes_for_unreadable_inodes() {
        let mut img = build_ext4_fixture();
        let directory_entry_inode = 12288 + 24;
        img[directory_entry_inode..directory_entry_inode + 4].copy_from_slice(&99u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4
            .list_children("")
            .expect_err("unreadable inode was downgraded to a fake node");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn reports_ext4_fscrypt_superblock_feature_without_claiming_all_files_are_encrypted() {
        let mut img = build_ext4_fixture();
        let feature_offset = 1024 + 0x60;
        img[feature_offset..feature_offset + 4]
            .copy_from_slice(&EXT4_FEATURE_INCOMPAT_ENCRYPT.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert!(ext4.has_encryption_feature());
        let file = ext4
            .list_children("")
            .unwrap()
            .into_iter()
            .find(|node| node.name == "test.txt")
            .expect("plain test file");
        assert!(!file.encrypted);
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
        let mode = Ext4Reader::inode_mode(&sym_inode).unwrap();
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
        ri[0x20..0x24].copy_from_slice(&0x0008_0000u32.to_le_bytes()); // EXT4_EXTENTS_FL
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
        fi[0x20..0x24].copy_from_slice(&0x0008_0000u32.to_le_bytes()); // EXT4_EXTENTS_FL
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

    // -----------------------------------------------------------------------
    // Hostile-input regression tests
    // -----------------------------------------------------------------------

    const FILE_INODE_OFFSET: usize = 8192 + 512; // inode 3 (test.txt)
    const SUBDIR_INODE_OFFSET: usize = 8192 + 768; // inode 4 (subdir)

    #[test]
    fn test_extent_header_rejects_depth_above_on_disk_maximum() {
        let mut header = [0u8; 12];
        header[0..2].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        header[6..8].copy_from_slice(&6u16.to_le_bytes());
        let error = Ext4ExtentHeader::parse(&header).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("depth"));

        header[6..8].copy_from_slice(&5u16.to_le_bytes());
        assert!(Ext4ExtentHeader::parse(&header).is_ok());
    }

    #[test]
    fn test_extent_tree_depth_above_max_rejected_on_read() {
        let mut img = build_ext4_fixture();
        img[FILE_INODE_OFFSET + 0x2E..FILE_INODE_OFFSET + 0x30]
            .copy_from_slice(&6u16.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.open_file("test.txt").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("depth"));
    }

    #[test]
    fn test_declared_size_beyond_filesystem_capacity_rejected() {
        let mut img = build_ext4_fixture();
        // The fixture holds 10 blocks of 4096 bytes (40960); claim one more.
        img[FILE_INODE_OFFSET + 0x04..FILE_INODE_OFFSET + 0x08]
            .copy_from_slice(&40961u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.open_file("test.txt").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("capacity"));

        let error = ext4.read_file_range("test.txt", 0, 16).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_full_read_is_bounded_by_declared_size() {
        let mut img = build_ext4_fixture();
        // The extent claims 9 blocks starting at block 4, running past the
        // 10-block filesystem; i_size stays 11, so only block 4 may be read.
        img[FILE_INODE_OFFSET + 0x38..FILE_INODE_OFFSET + 0x3A]
            .copy_from_slice(&9u16.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let mut file = ext4.open_file("test.txt").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "Hello World");
    }

    #[test]
    fn test_unwritten_extent_zero_fill_bounded_by_declared_size() {
        let mut img = build_ext4_fixture();
        img[FILE_INODE_OFFSET + 0x04..FILE_INODE_OFFSET + 0x08]
            .copy_from_slice(&16u32.to_le_bytes());
        // Unwritten extent claiming 32767 blocks (~128 MiB of zero-fill).
        img[FILE_INODE_OFFSET + 0x38..FILE_INODE_OFFSET + 0x3A]
            .copy_from_slice(&0xFFFFu16.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let mut file = ext4.open_file("test.txt").unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        assert_eq!(content, vec![0u8; 16]);
    }

    #[test]
    fn test_inodes_count_beyond_block_group_capacity_rejected() {
        let mut img = build_ext4_fixture();
        // 10 blocks / 32768 blocks-per-group yields a single block group with
        // 16 inodes; claiming 33 inodes is impossible geometry.
        img[1024..1024 + 0x04].copy_from_slice(&33u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let error = Ext4Reader::open(reader, 0).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("inodes count"));
    }

    #[test]
    fn test_open_file_requires_extents_flag() {
        let mut img = build_ext4_fixture();
        img[FILE_INODE_OFFSET + 0x20..FILE_INODE_OFFSET + 0x24]
            .copy_from_slice(&0u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.open_file("test.txt").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("does not use extents"));

        let error = ext4.read_file_range("test.txt", 0, 4).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_open_file_rejects_inline_data_with_typed_unsupported() {
        let mut img = build_ext4_fixture();
        img[FILE_INODE_OFFSET + 0x20..FILE_INODE_OFFSET + 0x24]
            .copy_from_slice(&0x1008_0000u32.to_le_bytes()); // EXTENTS | INLINE_DATA

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.open_file("test.txt").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn test_directory_listing_requires_extents_flag() {
        let mut img = build_ext4_fixture();
        img[SUBDIR_INODE_OFFSET + 0x20..SUBDIR_INODE_OFFSET + 0x24]
            .copy_from_slice(&0u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.list_children("subdir").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("does not use extents"));
    }

    #[test]
    fn test_block_to_offset_rejects_out_of_range_blocks() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        assert!(ext4.block_to_offset(0).is_ok()); // first_data_block == 0
        assert!(ext4.block_to_offset(9).is_ok());
        let error = ext4.block_to_offset(10).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("outside filesystem bounds"));
    }

    #[test]
    fn test_extent_data_block_outside_filesystem_rejected() {
        let mut img = build_ext4_fixture();
        img[FILE_INODE_OFFSET + 0x3C..FILE_INODE_OFFSET + 0x40]
            .copy_from_slice(&10u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.open_file("test.txt").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("outside filesystem bounds"));
    }

    #[test]
    fn test_inode_mode_rejects_short_input() {
        assert!(Ext4Reader::inode_mode(&[]).is_err());
        assert!(Ext4Reader::inode_mode(&[0xA4]).is_err());
        assert_eq!(Ext4Reader::inode_mode(&[0xA4, 0x81]).unwrap(), 0x81A4);
    }

    // -----------------------------------------------------------------------
    // Unknown incompat feature admission control
    // -----------------------------------------------------------------------

    #[test]
    fn test_superblock_rejects_unknown_incompat_features() {
        for bit in [0x0010u32, 0x0008] {
            // META_BG relocates group descriptors; JOURNAL_DEV moves the
            // journal to an external device. Both must fail closed.
            let mut img = build_ext4_fixture();
            img[1024 + 0x60..1024 + 0x64].copy_from_slice(&bit.to_le_bytes());

            let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
            let error = Ext4Reader::open(reader, 0).err().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::Unsupported);
            assert!(
                error.to_string().contains("incompat feature bits"),
                "unexpected error for incompat bit 0x{bit:04X}: {error}"
            );
        }
    }

    #[test]
    fn test_superblock_accepts_known_incompat_features() {
        let mut img = build_ext4_fixture();
        let known = EXT4_FEATURE_INCOMPAT_FILETYPE
            | EXT4_FEATURE_INCOMPAT_EXTENTS
            | EXT4_FEATURE_INCOMPAT_FLEX_BG
            | EXT4_FEATURE_INCOMPAT_LARGEDIR
            | EXT4_FEATURE_INCOMPAT_INLINE_DATA
            | EXT4_FEATURE_INCOMPAT_CASEFOLD;
        img[1024 + 0x60..1024 + 0x64].copy_from_slice(&known.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let children = ext4.list_children("").unwrap();
        assert_eq!(children.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Inline-data symlink targets
    // -----------------------------------------------------------------------

    fn build_inline_symlink_fixture() -> Vec<u8> {
        let mut img = build_ext4_fixture();
        // Inode 6: convert the fast symlink to inline-data xattr layout.
        let offset = 8192 + 5 * 256;
        let inode = &mut img[offset..offset + 256];
        inode[0x20..0x24].copy_from_slice(&EXT4_INLINE_DATA_FL.to_le_bytes());
        inode[0x28..0x2C].copy_from_slice(&EXT4_XATTR_MAGIC.to_le_bytes());
        // xattr entry at i_block+4: name "data", value at i_block offset 24.
        inode[0x2C] = 4; // e_name_len
        inode[0x2D] = EXT4_XATTR_INDEX_SYSTEM;
        inode[0x2E..0x30].copy_from_slice(&24u16.to_le_bytes()); // e_value_offs
        inode[0x30..0x34].copy_from_slice(&0u32.to_le_bytes()); // e_value_inum
        inode[0x34..0x38].copy_from_slice(&13u32.to_le_bytes()); // e_value_size
        inode[0x38..0x3C].copy_from_slice(&0u32.to_le_bytes()); // e_hash
        inode[0x3C..0x40].copy_from_slice(b"data");
        inode[0x40..0x4D].copy_from_slice(b"/usr/bin/perl");
        img
    }

    #[test]
    fn test_inline_data_symlink_target() {
        let img = build_inline_symlink_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let sym_inode = ext4.read_inode(6).unwrap();
        let target = ext4.read_symlink_target(&sym_inode).unwrap();
        assert_eq!(target, "/usr/bin/perl");
    }

    #[test]
    fn test_inline_data_symlink_rejects_bad_xattr_magic() {
        let mut img = build_inline_symlink_fixture();
        let offset = 8192 + 5 * 256;
        img[offset + 0x28..offset + 0x2C].copy_from_slice(&0u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let sym_inode = ext4.read_inode(6).unwrap();
        let error = ext4.read_symlink_target(&sym_inode).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("xattr magic"));
    }

    #[test]
    fn test_inline_data_symlink_rejects_value_outside_i_block() {
        let mut img = build_inline_symlink_fixture();
        let offset = 8192 + 5 * 256;
        // Claim the value lives past the 60-byte i_block area.
        img[offset + 0x34..offset + 0x38].copy_from_slice(&128u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let sym_inode = ext4.read_inode(6).unwrap();
        let error = ext4.read_symlink_target(&sym_inode).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("exceeds i_block"));
    }

    #[test]
    fn test_inline_data_symlink_rejects_external_value_inode() {
        let mut img = build_inline_symlink_fixture();
        let offset = 8192 + 5 * 256;
        // e_value_inum != 0 means the payload lives in another inode.
        img[offset + 0x30..offset + 0x34].copy_from_slice(&9u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let sym_inode = ext4.read_inode(6).unwrap();
        let error = ext4.read_symlink_target(&sym_inode).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }

    // -----------------------------------------------------------------------
    // Extent tree traversal: child depth validation and cycle detection
    // -----------------------------------------------------------------------

    /// Root directory (inode 2) uses a depth-2 tree: index block 5 -> leaf
    /// block 6 -> directory data block 3. `f.txt` (inode 3) uses a depth-1
    /// tree: leaf block 7 -> data block 4.
    fn build_depth_two_tree_image() -> Vec<u8> {
        let mut img = vec![0u8; 10 * 4096];
        let sb = &mut img[1024..2048];
        sb[0x00..0x04].copy_from_slice(&16u32.to_le_bytes());
        sb[0x04..0x08].copy_from_slice(&10u32.to_le_bytes());
        sb[0x14..0x18].copy_from_slice(&0u32.to_le_bytes());
        sb[0x18..0x1C].copy_from_slice(&2u32.to_le_bytes());
        sb[0x20..0x24].copy_from_slice(&32768u32.to_le_bytes());
        sb[0x28..0x2C].copy_from_slice(&16u32.to_le_bytes());
        sb[0x38..0x3A].copy_from_slice(&EXT4_MAGIC.to_le_bytes());
        sb[0x58..0x5A].copy_from_slice(&256u16.to_le_bytes());
        img[4096 + 0x08..4096 + 0x0C].copy_from_slice(&2u32.to_le_bytes());

        let ri = &mut img[8192 + 256..8192 + 512];
        ri[0x00..0x02].copy_from_slice(&0x41EDu16.to_le_bytes()); // dir
        ri[0x04..0x08].copy_from_slice(&4096u32.to_le_bytes()); // i_size
        ri[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes()); // i_blocks
        ri[0x20..0x24].copy_from_slice(&0x0008_0000u32.to_le_bytes()); // EXT4_EXTENTS_FL
        ri[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        ri[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        ri[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        ri[0x2E..0x30].copy_from_slice(&2u16.to_le_bytes()); // eh_depth=2
        ri[0x38..0x3C].copy_from_slice(&5u32.to_le_bytes()); // index -> block 5

        let idx = &mut img[5 * 4096..6 * 4096];
        idx[0x00..0x02].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        idx[0x02..0x04].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        idx[0x04..0x06].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        idx[0x06..0x08].copy_from_slice(&1u16.to_le_bytes()); // eh_depth=1
        idx[0x10..0x14].copy_from_slice(&6u32.to_le_bytes()); // ei_leaf -> block 6

        let leaf = &mut img[6 * 4096..7 * 4096];
        leaf[0x00..0x02].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        leaf[0x02..0x04].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        leaf[0x04..0x06].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        leaf[0x10..0x12].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        leaf[0x14..0x18].copy_from_slice(&3u32.to_le_bytes()); // ee_start_lo=3

        let rd = &mut img[3 * 4096..4 * 4096];
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

        let fi = &mut img[8192 + 512..8192 + 768];
        fi[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes());
        fi[0x04..0x08].copy_from_slice(&11u32.to_le_bytes());
        fi[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes());
        fi[0x20..0x24].copy_from_slice(&0x0008_0000u32.to_le_bytes());
        fi[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        fi[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        fi[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        fi[0x2E..0x30].copy_from_slice(&1u16.to_le_bytes()); // eh_depth=1
        fi[0x38..0x3C].copy_from_slice(&7u32.to_le_bytes()); // index -> block 7

        let fleaf = &mut img[7 * 4096..8 * 4096];
        fleaf[0x00..0x02].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        fleaf[0x02..0x04].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        fleaf[0x04..0x06].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        fleaf[0x10..0x12].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        fleaf[0x14..0x18].copy_from_slice(&4u32.to_le_bytes()); // ee_start_lo=4

        img[4 * 4096..4 * 4096 + 11].copy_from_slice(b"depth2 test");
        img
    }

    #[test]
    fn test_extent_tree_depth_two_reads_through_index_levels() {
        let img = build_depth_two_tree_image();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let children = ext4.list_children("").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "f.txt");

        let mut file = ext4.open_file("f.txt").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "depth2 test");
        assert_eq!(ext4.read_file_range("f.txt", 2, 4).unwrap(), b"pth2");
    }

    #[test]
    fn test_extent_tree_rejects_child_depth_mismatch() {
        // Interior index node claims depth 0 while the parent expects 1.
        let mut img = build_depth_two_tree_image();
        img[5 * 4096 + 0x06..5 * 4096 + 0x08].copy_from_slice(&0u16.to_le_bytes());
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.list_children("").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("does not match expected depth"));

        // Leaf node claims depth 1 while the parent expects 0.
        let mut img = build_depth_two_tree_image();
        img[6 * 4096 + 0x06..6 * 4096 + 0x08].copy_from_slice(&1u16.to_le_bytes());
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.list_children("").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("extent leaf node has depth"));
    }

    #[test]
    fn test_extent_tree_rejects_cyclic_index_reference() {
        // The depth-1 index node references itself, forming a cycle.
        let mut img = build_depth_two_tree_image();
        img[5 * 4096 + 0x10..5 * 4096 + 0x14].copy_from_slice(&5u32.to_le_bytes());
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.list_children("").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("referenced more than once"));
    }

    #[test]
    fn test_extent_tree_rejects_duplicate_index_reference() {
        // Both of f.txt's index entries point at the same leaf block.
        let mut img = build_depth_two_tree_image();
        let fi = 8192 + 512;
        img[fi + 0x2A..fi + 0x2C].copy_from_slice(&2u16.to_le_bytes()); // eh_entries=2
        img[fi + 0x40..fi + 0x44].copy_from_slice(&1u32.to_le_bytes()); // ei_block=1
        img[fi + 0x44..fi + 0x48].copy_from_slice(&7u32.to_le_bytes()); // -> block 7

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let error = ext4.open_file("f.txt").err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("referenced more than once"));
        let error = ext4.read_file_range("f.txt", 0, 4).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("referenced more than once"));
    }

    // -----------------------------------------------------------------------
    // Directory entry filtering
    // -----------------------------------------------------------------------

    #[test]
    fn test_directory_entry_with_zero_inode_is_filtered() {
        let mut img = build_ext4_fixture();
        // test.txt entry in the root directory block: inode field -> 0.
        img[3 * 4096 + 24..3 * 4096 + 28].copy_from_slice(&0u32.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let names: Vec<String> = ext4
            .list_children("")
            .unwrap()
            .into_iter()
            .map(|node| node.name)
            .collect();
        assert_eq!(names, vec!["subdir".to_string()]);
    }

    #[test]
    fn test_directory_entry_with_unaligned_rec_len_stops_parsing() {
        let mut img = build_ext4_fixture();
        // ext4 record lengths are always 4-byte aligned; 13 is corrupt and
        // the remaining entries (subdir) can no longer be trusted.
        img[3 * 4096 + 28..3 * 4096 + 30].copy_from_slice(&13u16.to_le_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert!(ext4.list_children("").unwrap().is_empty());
    }

    #[test]
    fn test_directory_entry_name_beyond_record_is_skipped() {
        let mut img = build_ext4_fixture();
        // test.txt's record is 24 bytes, but a name of 32 would spill into
        // the next entry; the entry is skipped while subdir still parses.
        img[3 * 4096 + 30] = 32;

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let names: Vec<String> = ext4
            .list_children("")
            .unwrap()
            .into_iter()
            .map(|node| node.name)
            .collect();
        assert_eq!(names, vec!["subdir".to_string()]);
    }
}
