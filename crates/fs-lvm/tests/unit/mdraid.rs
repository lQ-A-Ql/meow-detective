use super::*;
use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

struct MemoryReader {
    bytes: Cursor<Vec<u8>>,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        Self {
            bytes: Cursor::new(bytes),
            info: ReaderInfo {
                path: PathBuf::from("memory"),
                size,
                kind: "memory".to_string(),
            },
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.bytes.read(buffer)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.bytes.seek(position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn superblock(dev_number: u32) -> MdRaidSuperblock {
    MdRaidSuperblock {
        major_version: 1,
        feature_map: 0,
        array_uuid: [7; 16],
        name: "test".to_string(),
        level: 0,
        layout: 1,
        size_sectors: 0,
        chunk_sectors: 1,
        raid_disks: 2,
        data_offset_sectors: 1,
        data_size_sectors: 4,
        super_offset_sectors: 8,
        dev_number,
        device_uuid: [dev_number as u8; 16],
    }
}

#[test]
fn parses_v1_superblock_fields() {
    let mut bytes = [0u8; 256];
    bytes[0..4].copy_from_slice(&0xa92b4efcu32.to_le_bytes());
    bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
    bytes[16..32].fill(7);
    bytes[32..36].copy_from_slice(b"test");
    bytes[72..76].copy_from_slice(&0u32.to_le_bytes());
    bytes[76..80].copy_from_slice(&1u32.to_le_bytes());
    bytes[88..92].copy_from_slice(&1u32.to_le_bytes());
    bytes[92..96].copy_from_slice(&2u32.to_le_bytes());
    bytes[128..136].copy_from_slice(&1u64.to_le_bytes());
    bytes[136..144].copy_from_slice(&4u64.to_le_bytes());
    bytes[144..152].copy_from_slice(&8u64.to_le_bytes());
    bytes[160..164].copy_from_slice(&1u32.to_le_bytes());
    let parsed = parse_superblock(&bytes).unwrap();
    assert_eq!(parsed.array_uuid, [7; 16]);
    assert_eq!(parsed.name, "test");
    assert_eq!(parsed.raid_disks, 2);
    assert_eq!(parsed.chunk_bytes().unwrap(), 512);
    assert_eq!(parsed.dev_number, 1);
}

#[test]
fn raid0_reader_interleaves_member_chunks() {
    let mut first = vec![0; 512 + 2048];
    let mut second = vec![0; 512 + 2048];
    first[512..1024].fill(b'A');
    first[1024..1536].fill(b'C');
    second[512..1024].fill(b'B');
    second[1024..1536].fill(b'D');
    let reader = MdRaid0Reader::open(vec![
        MdRaidMember::new(superblock(0), Box::new(MemoryReader::new(first))),
        MdRaidMember::new(superblock(1), Box::new(MemoryReader::new(second))),
    ])
    .unwrap();
    let mut reader = reader;
    let mut output = vec![0; 2048];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(&output[0..512], vec![b'A'; 512].as_slice());
    assert_eq!(&output[512..1024], vec![b'B'; 512].as_slice());
    assert_eq!(&output[1024..1536], vec![b'C'; 512].as_slice());
    assert_eq!(&output[1536..2048], vec![b'D'; 512].as_slice());
}

#[test]
fn raid0_reader_rejects_missing_member() {
    let result = MdRaid0Reader::open(vec![MdRaidMember::new(
        superblock(0),
        Box::new(MemoryReader::new(vec![0; 4096])),
    )]);
    assert!(matches!(result, Err(MdRaidError::MemberCount { .. })));
}

#[test]
fn raid0_reader_uses_unequal_member_tail_zone() {
    let mut first = vec![0; 512 + 2048];
    let mut second = vec![0; 512 + 3072];
    first[512..1024].fill(b'A');
    first[1024..1536].fill(b'C');
    second[512..1024].fill(b'B');
    second[1024..1536].fill(b'D');
    second[2560..3072].fill(b'E');
    second[3072..3584].fill(b'F');
    let mut reader = MdRaid0Reader::open(vec![
        MdRaidMember::new(superblock(0), Box::new(MemoryReader::new(first))),
        MdRaidMember::new(
            MdRaidSuperblock {
                data_size_sectors: 6,
                ..superblock(1)
            },
            Box::new(MemoryReader::new(second)),
        ),
    ])
    .unwrap();
    let mut output = vec![0; 5120];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(&output[0..512], vec![b'A'; 512].as_slice());
    assert_eq!(&output[512..1024], vec![b'B'; 512].as_slice());
    assert_eq!(&output[1024..1536], vec![b'C'; 512].as_slice());
    assert_eq!(&output[1536..2048], vec![b'D'; 512].as_slice());
    assert_eq!(&output[4096..4608], vec![b'E'; 512].as_slice());
    assert_eq!(&output[4608..5120], vec![b'F'; 512].as_slice());
}
