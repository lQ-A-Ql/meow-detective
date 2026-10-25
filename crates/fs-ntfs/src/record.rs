use std::io;

/// Immutable access to a record after the common update-sequence validation.
pub struct ValidatedRecord<'a> {
    bytes: &'a [u8],
}

impl<'a> ValidatedRecord<'a> {
    pub fn new(bytes: &'a mut [u8], sector_size: usize) -> io::Result<Self> {
        crate::utils::apply_record_fixup(bytes, sector_size)?;
        Ok(Self { bytes })
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }

    pub fn mft_data(&self) -> io::Result<crate::MftDataAttribute> {
        crate::parse_mft_data_attribute(self.bytes)
    }
}
