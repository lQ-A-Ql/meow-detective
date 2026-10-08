#[path = "support/wof_xpress.rs"]
mod frames;
#[path = "support/wof_image.rs"]
mod image;

use evidence_core::{EvidenceReader, ReaderInfo};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct VirtualImage {
    header: Vec<u8>,
    frame: Vec<u8>,
    table_bytes: u64,
    payload_bytes: u64,
    position: u64,
    info: ReaderInfo,
    reads: Arc<AtomicUsize>,
}

impl Read for VirtualImage {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let length = output
            .len()
            .min(self.info.size.saturating_sub(self.position) as usize);
        for (index, byte) in output[..length].iter_mut().enumerate() {
            let position = self.position + index as u64;
            *byte = if position < 32768 {
                self.header[position as usize]
            } else {
                let local = position - 32768;
                if local < self.table_bytes {
                    let boundary = (local / 8 + 1) * self.frame.len() as u64;
                    boundary.to_le_bytes()[local as usize % 8]
                } else {
                    let payload = local - self.table_bytes;
                    if payload < self.payload_bytes - 1 {
                        self.frame[payload as usize % self.frame.len()]
                    } else {
                        b'Z'
                    }
                }
            };
        }
        self.position += length as u64;
        self.reads.fetch_add(length, Ordering::Relaxed);
        Ok(length)
    }
}

impl Seek for VirtualImage {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.info.size) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
        };
        self.position = u64::try_from(next)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "negative seek"))?;
        Ok(self.position)
    }
}

impl EvidenceReader for VirtualImage {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

#[test]
fn four_and_eight_gib_files_use_u64_offsets_and_bounded_reads() {
    for (size, compressed) in [((1u64 << 32) + 1, true), ((1u64 << 33) + 1, false)] {
        let full_chunks = (size - 1) / 4096;
        let frame = if compressed {
            frames::xpress(4096)
        } else {
            vec![b'A'; 4096]
        };
        let table_bytes = full_chunks * 8;
        let payload_bytes = full_chunks * frame.len() as u64 + 1;
        let ads_size = table_bytes + payload_bytes;
        let mut header = image::bytes(0, size, &[0]);
        header.truncate(32768);
        let named = 7168 + 128;
        let clusters = ads_size.div_ceil(512);
        header[named + 24..named + 32].copy_from_slice(&(clusters - 1).to_le_bytes());
        header[named + 40..named + 48].copy_from_slice(&(clusters * 512).to_le_bytes());
        header[named + 48..named + 56].copy_from_slice(&ads_size.to_le_bytes());
        header[named + 56..named + 64].copy_from_slice(&ads_size.to_le_bytes());
        let run = u16::from_le_bytes(header[named + 32..named + 34].try_into().unwrap()) as usize;
        header[named + run + 1..named + run + 5].copy_from_slice(&(clusters as u32).to_le_bytes());
        let reads = Arc::new(AtomicUsize::new(0));
        let source = VirtualImage {
            header,
            frame,
            table_bytes,
            payload_bytes,
            position: 0,
            info: ReaderInfo {
                path: "virtual-wof.raw".into(),
                size: 32768 + ads_size,
                kind: "virtual-test".into(),
            },
            reads: reads.clone(),
        };
        let fs = fs_ntfs::NtfsReader::open(Box::new(source), 0).unwrap();
        assert_eq!(fs.file_size_by_inode(6).unwrap(), Some(size));
        assert_eq!(fs.read_file_range_by_inode(6, size - 2, 2).unwrap(), b"AZ");
        let mut stream = fs.into_file_stream_by_inode(6).unwrap();
        stream.seek(SeekFrom::End(-1)).unwrap();
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        assert_eq!(byte[0], b'Z');
        stream.seek(SeekFrom::Start((1u64 << 31) + 123)).unwrap();
        stream.read_exact(&mut byte).unwrap();
        assert_eq!(byte[0], b'A');
        assert!(
            reads.load(Ordering::Relaxed) < 16 * 1024,
            "Large WOF files must not load their whole table or content"
        );
    }
}
