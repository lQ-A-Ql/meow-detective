use super::*;

use std::{
    io::{Cursor, Read, Seek, SeekFrom},
    time::Duration,
};

use sha2::{Digest, Sha256};

struct DelayedReader {
    inner: Cursor<Vec<u8>>,
}

impl Read for DelayedReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.inner.position() == 0 {
            std::thread::sleep(Duration::from_millis(20));
        }
        self.inner.read(output)
    }
}

impl Seek for DelayedReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _output: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("synthetic read failure"))
    }
}

struct PanickingReader;

impl Read for PanickingReader {
    fn read(&mut self, _output: &mut [u8]) -> std::io::Result<usize> {
        panic!("synthetic worker panic");
    }
}

impl Seek for PanickingReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        match position {
            SeekFrom::Start(offset) => Ok(offset),
            SeekFrom::End(_) => Ok(8192),
            SeekFrom::Current(_) => Ok(0),
        }
    }
}

impl Seek for FailingReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        match position {
            SeekFrom::Start(offset) => Ok(offset),
            SeekFrom::End(_) => Ok(8192),
            SeekFrom::Current(_) => Ok(0),
        }
    }
}

#[test]
fn parallel_copy_orders_chunks_and_hashes_the_logical_stream() {
    let source: Vec<u8> = (0..16_411).map(|index| (index % 251) as u8).collect();
    let readers = (0..2)
        .map(|_| {
            Box::new(DelayedReader {
                inner: Cursor::new(source.clone()),
            }) as Box<dyn evidence_core::ReadSeek + Send>
        })
        .collect();
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("parallel.bin");
    let mut updates = Vec::new();
    let mut callback = |update| updates.push(update);

    let result = copy_parallel_readers_to_destination(
        readers,
        source.len() as u64,
        1024,
        4,
        &destination,
        false,
        Some(&mut callback),
    )
    .unwrap();

    assert_eq!(std::fs::read(destination).unwrap(), source);
    assert_eq!(result.bytes_written, source.len() as u64);
    assert_eq!(result.sha256, hex::encode(Sha256::digest(&source)));
    assert!(updates
        .windows(2)
        .all(|pair| pair[0].bytes_written <= pair[1].bytes_written));
    assert_eq!(updates.last().unwrap().bytes_written, source.len() as u64);
}

#[test]
fn parallel_copy_failure_does_not_publish_partial_destination() {
    let readers = vec![
        Box::new(FailingReader) as Box<dyn evidence_core::ReadSeek + Send>,
        Box::new(FailingReader) as Box<dyn evidence_core::ReadSeek + Send>,
    ];
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("failed.bin");

    let error =
        copy_parallel_readers_to_destination(readers, 8192, 1024, 4, &destination, false, None)
            .unwrap_err();

    assert!(matches!(error, FileServiceError::Io(_)));
    assert!(!destination.exists());
    assert!(std::fs::read_dir(temporary.path())
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn parallel_copy_rejects_catalog_stream_size_mismatch() {
    let source = vec![0_u8; 4096];
    let readers = (0..2)
        .map(|_| Box::new(Cursor::new(source.clone())) as Box<dyn evidence_core::ReadSeek + Send>)
        .collect();
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("mismatch.bin");

    let error =
        copy_parallel_readers_to_destination(readers, 2048, 1024, 4, &destination, false, None)
            .unwrap_err();

    assert!(matches!(error, FileServiceError::Integrity(_)));
    assert!(!destination.exists());
}

#[test]
fn parallel_copy_worker_panic_does_not_publish_partial_destination() {
    let readers = vec![
        Box::new(PanickingReader) as Box<dyn evidence_core::ReadSeek + Send>,
        Box::new(PanickingReader) as Box<dyn evidence_core::ReadSeek + Send>,
    ];
    let temporary = tempfile::TempDir::new().unwrap();
    let destination = temporary.path().join("panic.bin");

    let error =
        copy_parallel_readers_to_destination(readers, 8192, 1024, 4, &destination, false, None)
            .unwrap_err();

    assert!(matches!(error, FileServiceError::Io(_)));
    assert!(!destination.exists());
    assert!(std::fs::read_dir(temporary.path())
        .unwrap()
        .next()
        .is_none());
}
