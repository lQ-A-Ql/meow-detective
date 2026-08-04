use std::io::{self, Read};
use std::sync::{Arc, Mutex};

use evidence_core::{FileSystemReader, FsNode};
use evidence_mount::MountFileHandle;

use super::{FilesystemRangeHandle, SharedFilesystem, READ_AHEAD_BYTES};

type ReadLog = Arc<Mutex<Vec<(String, u64, usize)>>>;

struct TrackingFilesystem {
    data: Vec<u8>,
    reads: ReadLog,
}

impl FileSystemReader for TrackingFilesystem {
    fn root(&self) -> io::Result<FsNode> {
        Err(io::ErrorKind::Unsupported.into())
    }

    fn list_children(&self, _path: &str) -> io::Result<Vec<FsNode>> {
        Err(io::ErrorKind::Unsupported.into())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Err(io::ErrorKind::Unsupported.into())
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        self.reads
            .lock()
            .expect("read log")
            .push((path.to_string(), offset, length));
        if path == "bad" {
            return Err(io::ErrorKind::NotFound.into());
        }
        let start = usize::try_from(offset).map_err(|_| io::ErrorKind::InvalidInput)?;
        let end = start.saturating_add(length).min(self.data.len());
        Ok(self.data.get(start..end).unwrap_or(&[]).to_vec())
    }

    fn data_source_name(&self) -> &str {
        "tracking"
    }
}

fn handle(size: usize) -> (FilesystemRangeHandle, ReadLog) {
    let reads = Arc::new(Mutex::new(Vec::new()));
    let filesystem: SharedFilesystem = Arc::new(Mutex::new(Box::new(TrackingFilesystem {
        data: (0..size).map(|value| (value % 251) as u8).collect(),
        reads: Arc::clone(&reads),
    })));
    (
        FilesystemRangeHandle::new(
            filesystem,
            vec!["bad".to_string(), "good".to_string()],
            size as u64,
        ),
        reads,
    )
}

#[test]
fn caches_repeated_reads_and_prefetches_only_after_sequential_access() {
    let (mut handle, reads) = handle(READ_AHEAD_BYTES * 2);

    let first = handle.read_at(0, 4).expect("first read");
    assert_eq!(first, vec![0, 1, 2, 3]);
    assert_eq!(handle.read_at(0, 4).expect("cached read"), first);
    assert_eq!(
        handle.read_at(4, 4).expect("sequential read"),
        vec![4, 5, 6, 7]
    );
    assert_eq!(
        handle.read_at(8, 4).expect("read-ahead hit"),
        vec![8, 9, 10, 11]
    );

    let reads = reads.lock().expect("read log");
    assert_eq!(reads.len(), 3);
    assert_eq!(reads[0], ("bad".to_string(), 0, 4));
    assert_eq!(reads[1], ("good".to_string(), 0, 4));
    assert_eq!(reads[2], ("good".to_string(), 4, READ_AHEAD_BYTES));
}

#[test]
fn random_reads_do_not_trigger_read_amplification() {
    let (mut handle, reads) = handle(1024);

    handle.read_at(500, 8).expect("first random read");
    handle.read_at(100, 8).expect("second random read");

    let reads = reads.lock().expect("read log");
    assert_eq!(reads.last(), Some(&("good".to_string(), 100, 8)));
}
