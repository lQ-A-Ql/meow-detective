use evidence_core::EvidenceReader;
use fs_ntfs::NtfsReader;
use std::io::{self, Read, Seek, SeekFrom};

struct MemoryReader {
    data: Vec<u8>,
    position: u64,
    info: evidence_core::ReaderInfo,
}

impl MemoryReader {
    fn new(data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            position: 0,
            info: evidence_core::ReaderInfo {
                path: "memory-ntfs".into(),
                size,
                kind: "test".to_string(),
            },
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let start = usize::try_from(self.position).unwrap_or(self.data.len());
        let end = start.saturating_add(buffer.len()).min(self.data.len());
        let length = end.saturating_sub(start);
        buffer[..length].copy_from_slice(&self.data[start..end]);
        self.position = self.position.saturating_add(length as u64);
        Ok(length)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => value as i128,
            SeekFrom::End(value) => self.data.len() as i128 + value as i128,
            SeekFrom::Current(value) => self.position as i128 + value as i128,
        };
        if next < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "negative seek in memory reader",
            ));
        }
        self.position = u64::try_from(next).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "seek exceeds memory reader")
        })?;
        Ok(self.position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

fn ntfs_fixture(sequence: u16, flags: u16) -> Vec<u8> {
    let cluster_size = 512usize;
    let mft_offset = 2 * cluster_size;
    let record_size = 1024usize;
    let record_offset = mft_offset + 5 * record_size;
    let mut image = vec![0u8; record_offset + record_size];

    image[3..11].copy_from_slice(b"NTFS    ");
    image[11..13].copy_from_slice(&512u16.to_le_bytes());
    image[13] = 1;
    image[0x30..0x38].copy_from_slice(&2u64.to_le_bytes());
    image[0x40] = (-10i8) as u8;

    image[mft_offset..mft_offset + 4].copy_from_slice(b"FILE");
    let record = &mut image[record_offset..record_offset + record_size];
    record[0..4].copy_from_slice(b"FILE");
    record[0x10..0x12].copy_from_slice(&sequence.to_le_bytes());
    record[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
    image
}

#[test]
fn sequence_change_rejects_persisted_deleted_candidate() {
    let reader = NtfsReader::open(Box::new(MemoryReader::new(ntfs_fixture(8, 0))), 0).unwrap();
    let error = reader
        .validate_deleted_file_record(5, 7)
        .expect_err("reused MFT slot must not be readable with an old sequence");
    assert!(error.to_string().contains("MFT sequence changed"));
}

#[test]
fn reactivated_record_rejects_persisted_deleted_candidate() {
    let reader = NtfsReader::open(Box::new(MemoryReader::new(ntfs_fixture(7, 1))), 0).unwrap();
    let error = reader
        .validate_deleted_file_record(5, 7)
        .expect_err("an active MFT record must not be exposed as deleted content");
    assert!(error.to_string().contains("active again"));
}
