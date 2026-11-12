use domain::FileEntryId;

use super::{ParallelSourceReaders, SourceReadContext, SourceReadFileHint};
use crate::file_service::{
    viewer::{
        descriptor_for_file_with_cache, open_range_content_for_descriptor_with_context,
        read_file_bytes_for_descriptor_with_context, RangeContentReader,
    },
    FileServiceError,
};

pub enum SourceExtractionMode {
    Reader(RangeContentReader),
    Parallel(ParallelSourceReaders),
    Chunked,
}

pub struct SourceExtractionPlan {
    pub size: u64,
    pub mode: SourceExtractionMode,
}

impl SourceReadContext<'_> {
    pub fn open_file_range_by_id(
        &mut self,
        file_id: &FileEntryId,
    ) -> Result<RangeContentReader, FileServiceError> {
        let descriptor = descriptor_for_file_with_cache(&mut *self, file_id)?;
        open_range_content_for_descriptor_with_context(self, &descriptor)
    }

    pub fn open_file_range_with_metadata(
        &mut self,
        hint: SourceReadFileHint,
    ) -> Result<RangeContentReader, FileServiceError> {
        if hint.data_source_id != *self.data_source_id {
            return Err(FileServiceError::security(
                "Source-read hint does not belong to the bound data source",
            ));
        }
        let Some(partition_index) = hint.partition_index else {
            return self.open_file_range_by_id(&hint.file_id);
        };
        let (source_kind, source_path) = self.source_location()?.clone();
        let descriptor =
            self.descriptor_for_hint(&hint, partition_index, source_kind, source_path)?;
        open_range_content_for_descriptor_with_context(self, &descriptor)
    }

    pub fn extraction_plan_by_id(
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

    pub fn read_extraction_chunk_by_id(
        &mut self,
        file_id: &FileEntryId,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, FileServiceError> {
        let descriptor = descriptor_for_file_with_cache(&mut *self, file_id)?;
        read_file_bytes_for_descriptor_with_context(self, &descriptor, offset, length)
    }
}
