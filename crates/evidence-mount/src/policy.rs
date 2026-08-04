use crate::MountError;

pub const DEFAULT_MAX_READ_LENGTH: usize = 1024 * 1024;
pub const DEFAULT_MAX_DIRECTORY_PAGE: u32 = 4096;
pub const DEFAULT_MAX_OPEN_HANDLES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountAccess {
    ReadOnly,
    Write,
}

impl MountAccess {
    pub fn validate(self) -> Result<(), MountError> {
        match self {
            Self::ReadOnly => Ok(()),
            Self::Write => Err(MountError::WriteDenied),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountReadPolicy {
    pub max_read_length: usize,
    pub max_directory_page: u32,
    pub max_open_handles: usize,
}

impl Default for MountReadPolicy {
    fn default() -> Self {
        Self {
            max_read_length: DEFAULT_MAX_READ_LENGTH,
            max_directory_page: DEFAULT_MAX_DIRECTORY_PAGE,
            max_open_handles: DEFAULT_MAX_OPEN_HANDLES,
        }
    }
}

impl MountReadPolicy {
    pub fn validate_read(
        &self,
        offset: u64,
        length: usize,
        size: u64,
    ) -> Result<usize, MountError> {
        if length > self.max_read_length {
            return Err(MountError::ReadLimit {
                requested: length,
                maximum: self.max_read_length,
            });
        }
        if offset > size {
            return Err(MountError::OffsetOutOfBounds { offset, size });
        }
        Ok(length.min(size.saturating_sub(offset) as usize))
    }

    pub fn validate_directory_page(&self, limit: u32) -> Result<(), MountError> {
        if limit == 0 {
            return Err(MountError::InvalidDirectoryLimit);
        }
        if limit > self.max_directory_page {
            return Err(MountError::DirectoryLimit {
                requested: limit,
                maximum: self.max_directory_page,
            });
        }
        Ok(())
    }
}
