use domain::FileEntryId;

use super::SourceReadContext;
use crate::file_service::{
    viewer::{
        descriptor_for_file_with_cache, open_range_content_for_descriptor_with_context,
        RangeContentReader,
    },
    FileServiceError,
};

impl SourceReadContext<'_> {
    pub(crate) fn open_file_range_by_id(
        &mut self,
        file_id: &FileEntryId,
    ) -> Result<RangeContentReader, FileServiceError> {
        let descriptor = descriptor_for_file_with_cache(&mut *self, file_id)?;
        open_range_content_for_descriptor_with_context(self, &descriptor)
    }
}
