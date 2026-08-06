use sha2::{Digest, Sha256};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::mpsc;

const PIPELINE_BUFFER_BYTES: usize = 8 * 1024 * 1024;
const PIPELINE_READER_THREADS: usize = 8;

struct ReaderLane {
    receiver: mpsc::Receiver<io::Result<Vec<u8>>>,
    recycle: mpsc::SyncSender<Vec<u8>>,
}

pub fn sha256_pipeline_worker_threads() -> usize {
    PIPELINE_READER_THREADS + 1
}

/// Hashes a file using bounded concurrent reads and one ordered SHA-256 chain.
pub fn sha256_file_pipelined_with_cancel(
    path: &Path,
    cancelled: &(dyn Fn() -> bool + Sync),
    mut on_progress: impl FnMut(u64),
) -> io::Result<Option<String>> {
    let length = std::fs::metadata(path)?.len();
    std::thread::scope(|scope| {
        let mut lanes = Vec::with_capacity(PIPELINE_READER_THREADS);
        let mut workers = Vec::with_capacity(PIPELINE_READER_THREADS);
        for lane in 0..PIPELINE_READER_THREADS {
            let (sender, receiver) = mpsc::sync_channel(0);
            let (recycle, recycled) = mpsc::sync_channel(0);
            lanes.push(ReaderLane { receiver, recycle });
            workers
                .push(scope.spawn(move || {
                    read_file_lane(path, length, lane, cancelled, sender, recycled)
                }));
        }
        let result = hash_reader_lanes(length, &lanes, cancelled, &mut on_progress);
        drop(lanes);
        let worker_result = join_readers(workers);
        match result {
            Ok(Some(digest)) => {
                worker_result?;
                Ok(Some(digest))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        }
    })
}

fn hash_reader_lanes(
    length: u64,
    lanes: &[ReaderLane],
    cancelled: &(dyn Fn() -> bool + Sync),
    on_progress: &mut impl FnMut(u64),
) -> io::Result<Option<String>> {
    let chunk_count = length.div_ceil(PIPELINE_BUFFER_BYTES as u64);
    let mut hasher = Sha256::new();
    let mut processed = 0u64;
    for index in 0..chunk_count {
        if cancelled() {
            return Ok(None);
        }
        let lane = &lanes[index as usize % lanes.len()];
        let buffer = lane.receiver.recv().map_err(|_| {
            io::Error::new(io::ErrorKind::UnexpectedEof, "SHA-256 reader stopped early")
        })??;
        hasher.update(&buffer);
        processed = processed.saturating_add(buffer.len() as u64);
        on_progress(processed);
        if lane.recycle.send(buffer).is_err() && index + 1 < chunk_count {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "SHA-256 reader buffer recycler stopped early",
            ));
        }
    }
    Ok(Some(hex::encode(hasher.finalize())))
}

fn read_file_lane(
    path: &Path,
    length: u64,
    lane: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
    sender: mpsc::SyncSender<io::Result<Vec<u8>>>,
    recycled: mpsc::Receiver<Vec<u8>>,
) -> io::Result<()> {
    let mut file = std::fs::File::open(path)?;
    let stride = PIPELINE_BUFFER_BYTES as u64 * PIPELINE_READER_THREADS as u64;
    let mut offset = PIPELINE_BUFFER_BYTES as u64 * lane as u64;
    let mut buffer = vec![0u8; PIPELINE_BUFFER_BYTES];
    while offset < length && !cancelled() {
        let read_length = usize::try_from((length - offset).min(PIPELINE_BUFFER_BYTES as u64))
            .map_err(|_| io::Error::other("SHA-256 read length exceeds usize"))?;
        buffer.resize(read_length, 0);
        file.seek(SeekFrom::Start(offset))?;
        if let Err(error) = file.read_exact(&mut buffer) {
            let _ = sender.send(Err(error));
            return Ok(());
        }
        if sender.send(Ok(buffer)).is_err() {
            return Ok(());
        }
        buffer = match recycled.recv() {
            Ok(buffer) => buffer,
            Err(_) => return Ok(()),
        };
        offset = offset.saturating_add(stride);
    }
    Ok(())
}

fn join_readers(workers: Vec<std::thread::ScopedJoinHandle<'_, io::Result<()>>>) -> io::Result<()> {
    for worker in workers {
        worker
            .join()
            .map_err(|_| io::Error::other("SHA-256 reader worker panicked"))??;
    }
    Ok(())
}
