use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

use crossbeam_channel::Sender;
use image_e01::E01Reader;
use persistence_sqlite::{DbError, DbResult};

use super::stream::{read_contiguous_ntfs_mft_stream, read_ntfs_mft_stream};

const MFT_CHUNK_RECORDS: u64 = 10_000;

pub(super) struct MftChunk {
    pub(super) data: Vec<u8>,
    pub(super) start_record: u64,
    pub(super) count: u64,
}

pub(super) struct MftReaderConfig {
    pub(super) e01_path: PathBuf,
    pub(super) volume_offset: u64,
    pub(super) mft_cluster: u64,
    pub(super) cluster_size: u64,
    pub(super) total_records: u64,
    pub(super) scanner_record_size: u32,
    pub(super) data_runs: Vec<(i64, u64)>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum MftReaderError {
    #[error("failed to open the evidence image ({kind:?})")]
    OpenEvidence { kind: std::io::ErrorKind },
    #[error("failed to read the MFT chunk starting at record {start_record} ({kind:?})")]
    ReadChunk {
        start_record: u64,
        kind: std::io::ErrorKind,
    },
    #[error("MFT enumeration was cancelled")]
    Cancelled,
    #[error("all MFT parser workers stopped before enumeration completed")]
    ParserChannelClosed,
}

pub(super) type MftReaderHandle = JoinHandle<Result<(), MftReaderError>>;

pub(super) fn spawn_reader(
    config: MftReaderConfig,
    chunk_tx: Sender<MftChunk>,
    processed: Arc<AtomicU64>,
    cancel: Option<Arc<AtomicBool>>,
    pipeline_stop: Arc<AtomicBool>,
) -> DbResult<MftReaderHandle> {
    std::thread::Builder::new()
        .name("mft-reader".into())
        .spawn(move || read_chunks(config, chunk_tx, processed, cancel, pipeline_stop))
        .map_err(|error| DbError::System(format!("Failed to spawn MFT reader: {error}")))
}

fn read_chunks(
    config: MftReaderConfig,
    chunk_tx: Sender<MftChunk>,
    processed: Arc<AtomicU64>,
    cancel: Option<Arc<AtomicBool>>,
    pipeline_stop: Arc<AtomicBool>,
) -> Result<(), MftReaderError> {
    let mut reader = E01Reader::open(&config.e01_path)
        .map_err(|error| MftReaderError::OpenEvidence { kind: error.kind() })?;
    let mut start_record = 0u64;
    while start_record < config.total_records {
        if pipeline_stop.load(Ordering::Relaxed)
            || cancel
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(MftReaderError::Cancelled);
        }
        let chunk_count = MFT_CHUNK_RECORDS.min(config.total_records - start_record);
        let mut data = vec![0u8; (chunk_count * config.scanner_record_size as u64) as usize];
        read_chunk(&mut reader, &config, start_record, &mut data).map_err(|error| {
            MftReaderError::ReadChunk {
                start_record,
                kind: error.kind(),
            }
        })?;
        chunk_tx
            .send(MftChunk {
                data,
                start_record,
                count: chunk_count,
            })
            .map_err(|_| MftReaderError::ParserChannelClosed)?;
        start_record += chunk_count;
        processed.store(start_record, Ordering::Relaxed);
    }
    Ok(())
}

fn read_chunk(
    reader: &mut E01Reader,
    config: &MftReaderConfig,
    start_record: u64,
    data: &mut [u8],
) -> std::io::Result<()> {
    let stream_offset = start_record * config.scanner_record_size as u64;
    if config.data_runs.is_empty() {
        read_contiguous_ntfs_mft_stream(
            reader,
            config.volume_offset,
            config.mft_cluster,
            config.cluster_size,
            stream_offset,
            data,
        )
    } else {
        read_ntfs_mft_stream(
            reader,
            config.volume_offset,
            config.cluster_size,
            &config.data_runs,
            stream_offset,
            data,
        )
    }
}
