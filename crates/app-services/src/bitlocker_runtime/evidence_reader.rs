use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use evidence_core::{EvidenceReader, PartitionWindowReader, ReaderInfo};
use volume_bitlocker::{read_volume_identities, BitLockerReader};

use super::{BitLockerRuntimeError, BitLockerUnlockRegistry};

pub struct BitLockerEvidenceReader {
    inner: BitLockerReader<PartitionWindowReader>,
    info: ReaderInfo,
}

impl BitLockerEvidenceReader {
    pub(crate) fn from_plaintext(
        inner: BitLockerReader<PartitionWindowReader>,
        source_info: &ReaderInfo,
    ) -> Self {
        Self {
            info: ReaderInfo {
                path: source_info.path.clone(),
                size: inner.len(),
                kind: "bitlocker-plaintext".to_string(),
            },
            inner,
        }
    }
}

impl Read for BitLockerEvidenceReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Seek for BitLockerEvidenceReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

impl EvidenceReader for BitLockerEvidenceReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        512
    }
}

pub fn open_registered_bitlocker_volume(
    reader: Box<dyn EvidenceReader>,
    partition_offset: u64,
    partition_length: Option<u64>,
    case_id: &str,
    data_source_id: &str,
    partition_index: usize,
    registry: &Arc<BitLockerUnlockRegistry>,
) -> Result<Box<dyn EvidenceReader>, BitLockerRuntimeError> {
    let source_info = reader.info().clone();
    let mut window = PartitionWindowReader::new(reader, partition_offset, partition_length)
        .map_err(BitLockerRuntimeError::InvalidWindow)?;
    let identities = read_volume_identities(&mut window)?;
    let registered =
        registry.resolve_for_identities(case_id, data_source_id, partition_index, &identities)?;
    let plaintext = BitLockerReader::new(registered.volume(), window)?;
    Ok(Box::new(BitLockerEvidenceReader::from_plaintext(
        plaintext,
        &source_info,
    )))
}
