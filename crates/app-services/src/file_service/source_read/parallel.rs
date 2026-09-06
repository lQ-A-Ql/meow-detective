use crate::file_service::{
    viewer::{exact_partition_candidate, open_ntfs_descriptor_stream, PreviewDescriptor},
    FileServiceError,
};

use super::{bitlocker, SourceReadContext};

const PARALLEL_EXTRACTION_MIN_BYTES: u64 = 512 * 1024 * 1024;
const PARALLEL_EXTRACTION_WORKERS: usize = 2;
const PARALLEL_EXTRACTION_MEMORY_RESERVATION_MB: u64 = 128;
pub(crate) const PARALLEL_EXTRACTION_CHUNK_BYTES: usize = 4 * 1024 * 1024;

pub(crate) struct ParallelSourceReaders {
    pub(crate) readers: Vec<Box<dyn evidence_core::ReadSeek + Send>>,
    pub(crate) chunk_bytes: usize,
    pub(crate) max_in_flight: usize,
}

impl SourceReadContext<'_> {
    pub(super) fn parallel_extraction_readers(
        &mut self,
        descriptor: &PreviewDescriptor,
    ) -> Result<Option<ParallelSourceReaders>, FileServiceError> {
        let rss_mb = crate::runtime_resources::current_rss_mb();
        let soft_limit_mb = crate::runtime_resources::default_memory_soft_limit_mb();
        let worker_count = parallel_worker_count(
            descriptor.entry_size,
            std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(1),
            rss_mb,
            soft_limit_mb,
        );
        if worker_count < 2
            || !matches!(
                descriptor.source_kind.as_str(),
                "e01" | "raw" | "local_disk"
            )
        {
            return Ok(None);
        }

        let candidate = exact_partition_candidate(descriptor)?;
        let mut readers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let (reader, filesystem_offset, filesystem_kind) =
                bitlocker::open_candidate_block_reader(self, descriptor, candidate)?;
            if !filesystem_kind.eq_ignore_ascii_case("NTFS") {
                return Ok(None);
            }
            let filesystem = fs_ntfs::NtfsReader::open(reader, filesystem_offset)?;
            let Some(reader) = open_ntfs_descriptor_stream(filesystem, descriptor, candidate)?
            else {
                return Ok(None);
            };
            readers.push(Box::new(reader) as Box<dyn evidence_core::ReadSeek + Send>);
        }

        let max_in_flight = worker_count.saturating_mul(2);
        tracing::info!(
            file_size = descriptor.entry_size,
            worker_count,
            chunk_bytes = PARALLEL_EXTRACTION_CHUNK_BYTES,
            max_in_flight,
            rss_mb,
            memory_soft_limit_mb = soft_limit_mb,
            "Using bounded parallel NTFS extraction"
        );
        Ok(Some(ParallelSourceReaders {
            readers,
            chunk_bytes: PARALLEL_EXTRACTION_CHUNK_BYTES,
            max_in_flight,
        }))
    }
}

fn parallel_worker_count(size: u64, cpu_count: usize, rss_mb: u64, soft_limit_mb: u64) -> usize {
    if size < PARALLEL_EXTRACTION_MIN_BYTES || cpu_count < 2 {
        return 1;
    }
    if rss_mb > 0
        && soft_limit_mb > 0
        && soft_limit_mb.saturating_sub(rss_mb) < PARALLEL_EXTRACTION_MEMORY_RESERVATION_MB
    {
        return 1;
    }
    PARALLEL_EXTRACTION_WORKERS.min(cpu_count)
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/source_read/parallel.rs"]
mod tests;
