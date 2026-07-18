use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, MutexGuard};

use evidence_core::{EvidenceReader, ReaderInfo};

use crate::error::{LvmError, Result};
use crate::thin::ThinMetadata;

type SharedReader = Arc<Mutex<Box<dyn EvidenceReader>>>;

/// Read-only dm-thin virtual block device.
///
/// The reader resolves `(device_id, thin_block)` through the thin-pool
/// metadata btree on demand. Unmapped thin blocks read as zeroes, matching
/// dm-thin semantics without materializing the full mapping table in memory.
pub struct ThinLvReader {
    metadata: ThinMetadata,
    data_reader: SharedReader,
    info: ReaderInfo,
    current_pos: u64,
    total_size: u64,
    device_id: u64,
    data_block_size: u64,
    preferred_read_granularity: usize,
}

impl ThinLvReader {
    pub fn new(
        metadata: ThinMetadata,
        data_reader: Box<dyn EvidenceReader>,
        lv_name: String,
        total_size: u64,
        device_id: u64,
    ) -> Result<Self> {
        let preferred_read_granularity = data_reader.preferred_read_granularity();
        let data_block_size = metadata.data_block_size_bytes()?;
        if data_block_size == 0 {
            return Err(metadata_error(
                "thin data block size resolved to zero bytes".to_string(),
            ));
        }
        if metadata.device_detail(device_id)?.is_none() {
            return Err(metadata_error(format!(
                "thin metadata has no device detail record for device_id {device_id}"
            )));
        }
        let info = ReaderInfo {
            path: std::path::PathBuf::from(format!("lvm+thin://{}", lv_name)),
            size: total_size,
            kind: "LVM2 Thin Logical Volume".to_string(),
        };

        Ok(Self {
            metadata,
            data_reader: Arc::new(Mutex::new(data_reader)),
            info,
            current_pos: 0,
            total_size,
            device_id,
            data_block_size,
            preferred_read_granularity,
        })
    }

    fn read_at(&self, mut logical_offset: u64, mut buf: &mut [u8]) -> std::io::Result<usize> {
        if logical_offset >= self.total_size {
            return Ok(0);
        }

        let mut total_read = 0usize;
        while !buf.is_empty() && logical_offset < self.total_size {
            let thin_block = logical_offset / self.data_block_size;
            let offset_in_block = logical_offset % self.data_block_size;
            let remaining_in_block = self.data_block_size - offset_in_block;
            let remaining_in_volume = self.total_size - logical_offset;
            let to_read = buf
                .len()
                .min(remaining_in_block.min(remaining_in_volume) as usize);
            if to_read == 0 {
                break;
            }

            match self.metadata.lookup_data_block(self.device_id, thin_block) {
                Ok(Some(mapping)) => {
                    let data_offset = mapping
                        .block
                        .checked_mul(self.data_block_size)
                        .and_then(|base| base.checked_add(offset_in_block))
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "thin data block offset overflows u64",
                            )
                        })?;
                    let mut reader = lock_data_reader(&self.data_reader)?;
                    reader.seek(SeekFrom::Start(data_offset))?;
                    reader.read_exact(&mut buf[..to_read])?;
                }
                Ok(None) => {
                    buf[..to_read].fill(0);
                }
                Err(error) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("thin metadata lookup failed: {error}"),
                    ));
                }
            }

            total_read += to_read;
            logical_offset += to_read as u64;
            let (_, rest) = buf.split_at_mut(to_read);
            buf = rest;
        }

        Ok(total_read)
    }
}

impl Read for ThinLvReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.read_at(self.current_pos, buf)?;
        self.current_pos += read as u64;
        Ok(read)
    }
}

impl Seek for ThinLvReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.current_pos = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                checked_relative_seek(self.total_size, offset, "thin volume end-relative seek")?
            }
            SeekFrom::Current(offset) => checked_relative_seek(
                self.current_pos,
                offset,
                "thin volume current-relative seek",
            )?,
        };
        Ok(self.current_pos)
    }
}

impl EvidenceReader for ThinLvReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        self.preferred_read_granularity
    }
}

fn metadata_error(message: String) -> LvmError {
    LvmError::MetadataParseError { line: 0, message }
}

fn lock_data_reader(
    reader: &SharedReader,
) -> std::io::Result<MutexGuard<'_, Box<dyn EvidenceReader>>> {
    reader
        .lock()
        .map_err(|_| std::io::Error::other("thin data reader lock poisoned"))
}

fn checked_relative_seek(base: u64, offset: i64, context: &str) -> std::io::Result<u64> {
    let absolute = i128::from(base) + i128::from(offset);
    if absolute < 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{context} moves before start of thin volume"),
        ));
    }
    u64::try_from(absolute).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{context} exceeds thin volume address space"),
        )
    })
}

#[cfg(test)]
#[path = "../tests/unit/thin_reader.rs"]
mod tests;
