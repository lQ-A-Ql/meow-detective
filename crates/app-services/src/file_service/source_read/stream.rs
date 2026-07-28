use domain::FileEntryId;

use super::ParallelSourceReaders;
use super::SourceReadContext;
use crate::file_service::{
    viewer::{
        descriptor_for_file_with_cache, open_range_content_for_descriptor_with_context,
        read_file_bytes_for_descriptor_with_context, RangeContentReader,
    },
    FileServiceError,
};

pub(crate) enum SourceExtractionMode {
    Reader(RangeContentReader),
    Parallel(ParallelSourceReaders),
    Chunked,
}

pub(crate) struct SourceExtractionPlan {
    pub(crate) size: u64,
    pub(crate) mode: SourceExtractionMode,
}

impl SourceReadContext<'_> {
    pub(crate) fn open_file_range_by_id(
        &mut self,
        file_id: &FileEntryId,
    ) -> Result<RangeContentReader, FileServiceError> {
        let descriptor = descriptor_for_file_with_cache(&mut *self, file_id)?;
        open_range_content_for_descriptor_with_context(self, &descriptor)
    }

    pub(crate) fn extraction_plan_by_id(
        &mut self,
        file_id: &FileEntryId,
    ) -> Result<SourceExtractionPlan, FileServiceError> {
        let descriptor = descriptor_for_file_with_cache(&mut *self, file_id)?;
        let mode = if descriptor.source_kind == "ceph_fs" {
            SourceExtractionMode::Chunked
        } else if let Some(readers) = self.parallel_extraction_readers(&descriptor)? {
            SourceExtractionMode::Parallel(readers)
        } else {
            SourceExtractionMode::Reader(open_range_content_for_descriptor_with_context(
                self,
                &descriptor,
            )?)
        };
        Ok(SourceExtractionPlan {
            size: descriptor.entry_size,
            mode,
        })
    }

    pub(crate) fn read_extraction_chunk_by_id(
        &mut self,
        file_id: &FileEntryId,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, FileServiceError> {
        let descriptor = descriptor_for_file_with_cache(&mut *self, file_id)?;
        read_file_bytes_for_descriptor_with_context(self, &descriptor, offset, length)
    }
}
