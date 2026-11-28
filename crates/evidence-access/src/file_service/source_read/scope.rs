use std::io::{self, Read, Seek, SeekFrom};

use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};
use source_kernel::source_runtime::read_scope::SourceReadScope;

use super::SourceReadContext;
use crate::file_service::{viewer::RangeContentReader, FileServiceError};

impl SourceReadContext<'_> {
    pub(super) fn wrap_content(
        &mut self,
        reader: Box<dyn Read>,
    ) -> Result<Box<dyn Read>, FileServiceError> {
        self.validate_scope()?;
        Ok(match &self.read_scope {
            Some(scope) => Box::new(ScopedFileReader {
                inner: reader,
                scope: scope.clone(),
            }),
            None => reader,
        })
    }

    pub(super) fn wrap_parallel_reader(
        &mut self,
        reader: Box<dyn evidence_core::ReadSeek + Send>,
    ) -> Result<Box<dyn evidence_core::ReadSeek + Send>, FileServiceError> {
        self.validate_scope()?;
        Ok(match &self.read_scope {
            Some(scope) => Box::new(ScopedFileReader {
                inner: reader,
                scope: scope.clone(),
            }),
            None => reader,
        })
    }

    pub(super) fn validate_scope(&mut self) -> Result<(), FileServiceError> {
        if let Some(scope) = &self.read_scope {
            return scope.validate().map_err(Into::into);
        }
        let storage = DataSourceRepo::new(self.case_conn)
            .find_storage(self.data_source_id)?
            .ok_or_else(|| FileServiceError::not_found("source registration is missing"))?;
        if matches!(storage.import_state.as_str(), "pending" | "importing") {
            return Ok(());
        }
        let (reference, _) = SourceRevisionRepo::new(self.case_conn)
            .current(&self.case_id.0, &self.data_source_id.0)?;
        self.read_scope = Some(SourceReadScope::acquire(self.case_root, &reference)?);
        Ok(())
    }

    pub(super) fn wrap_range(
        &mut self,
        reader: RangeContentReader,
    ) -> Result<RangeContentReader, FileServiceError> {
        self.validate_scope()?;
        let Some(scope) = &self.read_scope else {
            return Ok(reader);
        };
        Ok(match reader {
            RangeContentReader::Seekable(inner) => {
                RangeContentReader::Seekable(Box::new(ScopedFileReader {
                    inner,
                    scope: scope.clone(),
                }))
            }
            RangeContentReader::Streaming(inner) => {
                RangeContentReader::Streaming(Box::new(ScopedFileReader {
                    inner,
                    scope: scope.clone(),
                }))
            }
        })
    }
}

struct ScopedFileReader<Reader> {
    inner: Reader,
    scope: SourceReadScope,
}
impl<Reader: Read> Read for ScopedFileReader<Reader> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.scope.validate()?;
        let count = self.inner.read(bytes)?;
        if let Err(error) = self.scope.validate() {
            bytes[..count].fill(0);
            return Err(error);
        }
        Ok(count)
    }
}
impl<Reader: Seek> Seek for ScopedFileReader<Reader> {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        self.scope.validate()?;
        let position = self.inner.seek(target)?;
        self.scope.validate()?;
        Ok(position)
    }
}
