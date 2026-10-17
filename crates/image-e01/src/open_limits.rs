//! Allocation bounds for the metadata parse performed before any data reads.
use std::io;
pub(crate) const MAX_METADATA_BYTES: usize = 255 * 1024 * 1024;
pub(crate) const MAX_SECTION_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_SECTIONS: usize = 100_000;
pub(crate) const MAX_SEGMENTS: u32 = 512;

#[derive(Default)]
pub(crate) struct OpenBudget {
    pub sections: usize,
    pub content: usize,
}
impl OpenBudget {
    pub fn section(&mut self) -> io::Result<()> {
        self.sections += 1;
        if self.sections > MAX_SECTIONS {
            return Err(limit());
        }
        Ok(())
    }
    pub fn content(&mut self, bytes: usize) -> io::Result<()> {
        self.content = self.content.saturating_add(bytes);
        if self.content > MAX_SECTION_BYTES {
            return Err(limit());
        }
        Ok(())
    }
}
pub(crate) fn limit() -> io::Error {
    io::Error::new(
        io::ErrorKind::OutOfMemory,
        "E01 metadata exceeds the bounded open-plan capacity",
    )
}
