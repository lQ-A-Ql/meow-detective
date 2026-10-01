//! Seek and exact-read error mapping for evidence discovery.
use crate::{BitLockerError, Result};
use std::io::{Read, Seek, SeekFrom};
/// Seeks, mapping the failure onto the evidence-read error.
fn seek_to<R: Seek>(reader: &mut R, offset: u64) -> Result<()> {
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|source| BitLockerError::EvidenceRead { offset, source })?;
    Ok(())
}

/// Fills `buf` completely, treating a short read as a failure.
pub(super) fn read_exact_at<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    buf: &mut [u8],
) -> Result<()> {
    seek_to(reader, offset)?;
    reader
        .read_exact(buf)
        .map_err(|source| BitLockerError::EvidenceRead { offset, source })
}
