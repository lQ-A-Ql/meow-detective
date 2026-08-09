use super::*;

use std::io::{Read, Seek, SeekFrom};

use evidence_core::ReaderInfo;

struct FakeReader {
    data: Vec<u8>,
    pos: u64,
    info: ReaderInfo,
}

impl FakeReader {
    fn new(data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            pos: 0,
            info: ReaderInfo {
                path: std::path::PathBuf::from("fake-ext4"),
                size,
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

impl evidence_core::EvidenceReader for FakeReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn open(image: Vec<u8>) -> crate::Ext4Reader {
    let reader: Box<dyn evidence_core::EvidenceReader> = Box::new(FakeReader::new(image));
    crate::Ext4Reader::open(reader, 0).unwrap()
}

#[test]
fn maps_a_regular_file_to_its_physical_block() {
    let fs = open(testing::builders::ext4::linux_root_ext4_image());
    let extents = fs.file_extent_map("etc/os-release").unwrap();
    assert_eq!(extents.len(), 1);
    assert_eq!(extents[0].logical_offset, 0);
    assert_eq!(extents[0].volume_offset, 5 * 4096);
    assert_eq!(extents[0].length, 4096);
    let expected_size =
        b"NAME=\"CentOS Linux\"\nID=\"centos\"\nPRETTY_NAME=\"CentOS Linux 7 (Core)\"\n".len()
            as u64;
    assert_eq!(
        fs.file_size_by_path("etc/os-release").unwrap(),
        expected_size
    );
}

#[test]
fn reports_the_inode_table_position() {
    let fs = open(testing::builders::ext4::linux_root_ext4_image());
    let offset = fs.inode_source_offset("etc/shadow").unwrap();
    // The builder's inode table starts at block 2 with 256-byte inodes;
    // shadow is inode 10.
    assert_eq!(offset, 2 * 4096 + 9 * 256);
}

#[test]
fn refuses_directories_and_missing_files() {
    let fs = open(testing::builders::ext4::linux_root_ext4_image());
    assert!(fs.file_extent_map("etc").is_err());
    assert!(fs.file_extent_map("etc/nope").is_err());
}

#[test]
fn refuses_files_without_the_extents_flag() {
    let mut image = testing::builders::ext4::linux_root_ext4_image();
    // Clear EXT4_EXTENTS_FL on the shadow inode (inode 10, i_flags at 0x20).
    let inode = 2 * 4096 + 9 * 256;
    image[inode + 0x20..inode + 0x24].copy_from_slice(&0u32.to_le_bytes());
    let fs = open(image);
    let error = fs.file_extent_map("etc/shadow").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}
