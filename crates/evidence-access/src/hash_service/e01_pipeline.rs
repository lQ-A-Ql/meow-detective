use image_e01::E01Reader;
use infrastructure::hashing;
use std::io;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};

const MAX_LANE_COUNT: usize = 32;
const BUFFER_BYTES: usize = 2 * 1024 * 1024;
const LANE_WORKING_SET_MIB: u64 = 4;

pub(super) struct E01Digest {
    pub digest: String,
    pub worker_threads: usize,
}

struct ReaderLane {
    receiver: mpsc::Receiver<io::Result<Vec<u8>>>,
    recycle: mpsc::SyncSender<Vec<u8>>,
}

pub(super) fn digest_logical(
    reader: E01Reader,
    length: u64,
    algorithm: hashing::HashAlgorithm,
    cancelled: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> io::Result<Option<E01Digest>> {
    if length == 0 {
        let digest = hashing::digest_reader_with_cancel(
            &mut { reader },
            algorithm,
            || cancelled.load(Ordering::Acquire),
            |processed| progress(processed, 0),
        )?
        .ok_or_else(|| io::Error::new(io::ErrorKind::Interrupted, "digest cancelled"))?;
        return Ok(Some(E01Digest {
            digest,
            worker_threads: 1,
        }));
    }
    let available = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let lane_count = digest_lane_count(
        length,
        available,
        crate::runtime_resources::current_rss_mb(),
        crate::runtime_resources::default_memory_soft_limit_mb(),
    );
    let mut readers = Vec::with_capacity(lane_count);
    readers.push(reader.without_chunk_cache());
    for _ in 1..lane_count {
        readers.push(readers[0].try_clone()?.without_chunk_cache());
    }

    std::thread::scope(|scope| {
        let mut lanes = Vec::with_capacity(lane_count);
        let mut workers = Vec::with_capacity(lane_count);
        for (lane, reader) in readers.into_iter().enumerate() {
            let (sender, receiver) = mpsc::sync_channel(1);
            let (recycle, recycled) = mpsc::sync_channel(0);
            lanes.push(ReaderLane { receiver, recycle });
            workers.push(scope.spawn(move || {
                read_lane(
                    reader, length, lane, lane_count, cancelled, sender, recycled,
                )
            }));
        }
        let result = digest_from_lanes(&lanes, length, algorithm, cancelled, progress);
        drop(lanes);
        let worker_result = join_workers(workers);
        match result {
            Ok(Some(digest)) => {
                worker_result?;
                Ok(Some(E01Digest {
                    digest,
                    worker_threads: lane_count,
                }))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        }
    })
}

pub(super) fn digest_lane_count(
    length: u64,
    available: usize,
    rss_mb: u64,
    soft_limit_mb: u64,
) -> usize {
    let chunk_count = length.div_ceil(BUFFER_BYTES as u64);
    let cpu_limit = available.saturating_mul(2).min(MAX_LANE_COUNT);
    let memory_limit = if rss_mb > 0 && soft_limit_mb > rss_mb {
        usize::try_from((soft_limit_mb - rss_mb) / LANE_WORKING_SET_MIB).unwrap_or(usize::MAX)
    } else if rss_mb > 0 && soft_limit_mb > 0 {
        1
    } else {
        MAX_LANE_COUNT
    };
    usize::try_from(chunk_count.min(MAX_LANE_COUNT as u64))
        .unwrap_or(1)
        .min(cpu_limit)
        .min(memory_limit)
        .max(1)
}

fn digest_from_lanes(
    lanes: &[ReaderLane],
    length: u64,
    algorithm: hashing::HashAlgorithm,
    cancelled: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> io::Result<Option<String>> {
    let chunk_count = length.div_ceil(BUFFER_BYTES as u64);
    let mut hasher = hashing::StreamingHasher::new(algorithm);
    let mut processed = 0u64;
    for chunk_index in 0..chunk_count {
        if cancelled.load(Ordering::Acquire) {
            return Ok(None);
        }
        let lane = lanes
            .get(chunk_index as usize % lanes.len())
            .ok_or_else(|| io::Error::other("E01 digest lane is missing"))?;
        let buffer = match lane.receiver.recv() {
            Ok(result) => result?,
            Err(_) if cancelled.load(Ordering::Acquire) => return Ok(None),
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "E01 digest reader stopped early",
                ));
            }
        };
        let offset = chunk_index
            .checked_mul(BUFFER_BYTES as u64)
            .ok_or_else(|| io::Error::other("E01 digest offset overflow"))?;
        let expected = usize::try_from((length - offset).min(BUFFER_BYTES as u64))
            .map_err(|_| io::Error::other("E01 digest chunk length exceeds usize"))?;
        if buffer.len() != expected {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "E01 digest chunk length mismatch",
            ));
        }
        hasher.update(&buffer);
        processed = processed.saturating_add(buffer.len() as u64);
        progress(processed.min(length), length);
        lane.recycle.send(buffer).map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "E01 digest buffer recycler stopped early",
            )
        })?;
    }
    Ok(Some(hasher.finalize()))
}

fn read_lane(
    mut reader: E01Reader,
    length: u64,
    lane: usize,
    lane_count: usize,
    cancelled: &AtomicBool,
    sender: mpsc::SyncSender<io::Result<Vec<u8>>>,
    recycled: mpsc::Receiver<Vec<u8>>,
) -> io::Result<()> {
    let stride = (BUFFER_BYTES * lane_count) as u64;
    let mut offset = (BUFFER_BYTES * lane) as u64;
    let mut buffers = [Some(vec![0u8; BUFFER_BYTES]), Some(vec![0u8; BUFFER_BYTES])];
    let mut slot = 0usize;
    while offset < length && !cancelled.load(Ordering::Acquire) {
        let read_length = usize::try_from((length - offset).min(BUFFER_BYTES as u64))
            .map_err(|_| io::Error::other("E01 digest read length exceeds usize"))?;
        let buffer = buffers[slot].as_mut().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "E01 digest buffer is unavailable",
            )
        })?;
        buffer.resize(read_length, 0);
        reader.read_exact_at(offset, buffer)?;
        if sender
            .send(Ok(buffers[slot].take().expect("E01 digest buffer")))
            .is_err()
        {
            return Ok(());
        }
        slot ^= 1;
        if buffers[slot].is_none() {
            buffers[slot] = match recycled.recv() {
                Ok(buffer) => Some(buffer),
                Err(_) => return Ok(()),
            };
        }
        offset = offset.saturating_add(stride);
    }
    let _ = recycled.recv();
    Ok(())
}

fn join_workers(workers: Vec<std::thread::ScopedJoinHandle<'_, io::Result<()>>>) -> io::Result<()> {
    for worker in workers {
        worker
            .join()
            .map_err(|_| io::Error::other("E01 digest reader worker panicked"))??;
    }
    Ok(())
}
