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
}

impl ThinLvReader {
    pub fn new(
        metadata: ThinMetadata,
        data_reader: Box<dyn EvidenceReader>,
        lv_name: String,
        total_size: u64,
        device_id: u64,
    ) -> Result<Self> {
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
}

fn metadata_error(message: String) -> LvmError {
    LvmError::MetadataParseError { line: 0, message }
}

fn lock_data_reader(
    reader: &SharedReader,
) -> std::io::Result<MutexGuard<'_, Box<dyn EvidenceReader>>> {
    reader.lock().map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::Other, "thin data reader lock poisoned")
    })
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
mod tests {
    use super::*;

    const BLOCK_SIZE: usize = 4096;

    struct FakeReader {
        data: Vec<u8>,
        pos: u64,
        info: ReaderInfo,
    }

    impl FakeReader {
        fn new(data: Vec<u8>, kind: &str) -> Self {
            Self {
                info: ReaderInfo {
                    path: std::path::PathBuf::from(kind),
                    size: data.len() as u64,
                    kind: kind.to_string(),
                },
                data,
                pos: 0,
            }
        }
    }

    impl Read for FakeReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let start = self.pos as usize;
            let end = start.saturating_add(buf.len()).min(self.data.len());
            let read = end.saturating_sub(start);
            buf[..read].copy_from_slice(&self.data[start..end]);
            self.pos += read as u64;
            Ok(read)
        }
    }

    impl Seek for FakeReader {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.pos = match pos {
                SeekFrom::Start(offset) => offset,
                SeekFrom::End(offset) => (self.data.len() as i64 + offset).max(0) as u64,
                SeekFrom::Current(offset) => (self.pos as i64 + offset).max(0) as u64,
            };
            Ok(self.pos)
        }
    }

    impl EvidenceReader for FakeReader {
        fn info(&self) -> &ReaderInfo {
            &self.info
        }
    }

    #[test]
    fn thin_reader_maps_allocated_blocks_and_zero_fills_unmapped_blocks() {
        let metadata = build_thin_metadata();
        let mut data = vec![0u8; 4 * 512];
        data[2 * 512..2 * 512 + 11].copy_from_slice(b"THIN-BLOCK0");

        let thin_metadata =
            ThinMetadata::open(Box::new(FakeReader::new(metadata, "thin-metadata"))).unwrap();
        let data_reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data, "thin-data"));
        let mut reader =
            ThinLvReader::new(thin_metadata, data_reader, "thin-root".to_string(), 1024, 7)
                .unwrap();

        let mut allocated = [0u8; 11];
        reader.read_exact(&mut allocated).unwrap();
        assert_eq!(&allocated, b"THIN-BLOCK0");

        reader.seek(SeekFrom::Start(512)).unwrap();
        let mut unmapped = [0xAAu8; 16];
        reader.read_exact(&mut unmapped).unwrap();
        assert_eq!(unmapped, [0u8; 16]);
    }

    fn build_thin_metadata() -> Vec<u8> {
        let mut metadata = vec![0u8; 4 * BLOCK_SIZE];
        let superblock = &mut metadata[0..BLOCK_SIZE];
        superblock[8..16].copy_from_slice(&0u64.to_le_bytes());
        superblock[32..40].copy_from_slice(&27_022_010u64.to_le_bytes());
        superblock[40..44].copy_from_slice(&1u32.to_le_bytes());
        superblock[48..56].copy_from_slice(&1u64.to_le_bytes());
        superblock[320..328].copy_from_slice(&1u64.to_le_bytes());
        superblock[328..336].copy_from_slice(&2u64.to_le_bytes());
        superblock[336..340].copy_from_slice(&1u32.to_le_bytes());
        superblock[340..344].copy_from_slice(&8u32.to_le_bytes());
        superblock[344..352].copy_from_slice(&4u64.to_le_bytes());

        write_leaf_node(&mut metadata, 1, 7, 8, &3u64.to_le_bytes());
        let mut detail = [0u8; 24];
        detail[0..8].copy_from_slice(&1u64.to_le_bytes());
        detail[8..16].copy_from_slice(&1u64.to_le_bytes());
        write_leaf_node(&mut metadata, 2, 7, 24, &detail);
        let block_time = 2u64 << 24;
        write_leaf_node(&mut metadata, 3, 0, 8, &block_time.to_le_bytes());
        metadata
    }

    fn write_leaf_node(metadata: &mut [u8], block: u64, key: u64, value_size: u32, value: &[u8]) {
        let start = block as usize * BLOCK_SIZE;
        let node = &mut metadata[start..start + BLOCK_SIZE];
        let max_entries = 3u32;
        node[4..8].copy_from_slice(&2u32.to_le_bytes());
        node[8..16].copy_from_slice(&block.to_le_bytes());
        node[16..20].copy_from_slice(&1u32.to_le_bytes());
        node[20..24].copy_from_slice(&max_entries.to_le_bytes());
        node[24..28].copy_from_slice(&value_size.to_le_bytes());
        node[32..40].copy_from_slice(&key.to_le_bytes());
        let value_offset = 32 + max_entries as usize * 8;
        node[value_offset..value_offset + value.len()].copy_from_slice(value);
    }
}
