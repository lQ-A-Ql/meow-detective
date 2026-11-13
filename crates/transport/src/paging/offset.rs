use std::fmt;

pub const MAX_PAGE_OFFSET: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetLimitError {
    pub offset: u64,
}

impl fmt::Display for OffsetLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "offset {} exceeds maximum {}; use nextCursor to continue pagination",
            self.offset, MAX_PAGE_OFFSET
        )
    }
}

impl std::error::Error for OffsetLimitError {}

pub fn validate_page_offset(offset: u64) -> Result<(), OffsetLimitError> {
    if offset > MAX_PAGE_OFFSET {
        return Err(OffsetLimitError { offset });
    }
    Ok(())
}
