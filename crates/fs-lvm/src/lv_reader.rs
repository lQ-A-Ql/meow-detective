/// LV virtual block device — implements Read + Seek + EvidenceReader.
///
/// Translates logical byte offsets within a logical volume to physical
/// byte offsets on the underlying PV, using a pre-computed extent map.
///
/// Usage pattern (matching Ext4Reader/XfsReader/BtrfsReader):
/// ```ignore
/// let lv = LvReader::new(device_reader, "root", lv_size, extent_map)?;
/// let lv_box: Box<dyn EvidenceReader> = Box::new(lv);
/// let ext4 = Ext4Reader::open(lv_box, 0)?; // offset = 0, LV is a clean block device
/// ```
use std::io::{Read, Seek, SeekFrom};

use evidence_core::EvidenceReader;
use evidence_core::ReaderInfo;

use crate::segment::LvExtent;

type SharedReader = std::sync::Arc<std::sync::Mutex<Box<dyn EvidenceReader>>>;

/// Read-only access to a logical volume block device.
pub struct LvReader {
    /// Underlying PV readers in the same order as the VG metadata PV list.
    device_readers: Vec<SharedReader>,
    /// Metadata about this logical volume.
    info: ReaderInfo,
    /// Pre-computed LE → physical offset mapping.
    extent_map: Vec<LvExtent>,
    /// Current read position within the logical volume.
    current_pos: u64,
    /// Total size of this logical volume in bytes.
    total_size: u64,
    /// Read granularity inherited from the underlying evidence readers.
    preferred_read_granularity: usize,
}

impl LvReader {
    /// Create a new logical volume reader (owns the device reader).
    ///
    /// `device_reader`: the underlying disk image reader for the physical volume.
    /// `lv_name`: human-readable name (used in ReaderInfo).
    /// `total_size`: total byte size of this logical volume.
    /// `extent_map`: pre-computed logical→physical extent mapping.
    pub fn new(
        device_reader: Box<dyn EvidenceReader>,
        lv_name: String,
        total_size: u64,
        extent_map: Vec<LvExtent>,
    ) -> Self {
        Self::new_shared(
            vec![std::sync::Arc::new(std::sync::Mutex::new(device_reader))],
            lv_name,
            total_size,
            extent_map,
        )
    }

    /// Create a logical volume reader with shared PV readers.
    /// Enables opening multiple LVs from one pool without consuming the reader.
    pub fn new_shared(
        device_readers: Vec<SharedReader>,
        lv_name: String,
        total_size: u64,
        extent_map: Vec<LvExtent>,
    ) -> Self {
        let preferred_read_granularity = device_readers
            .iter()
            .filter_map(|reader| {
                reader
                    .lock()
                    .ok()
                    .map(|reader| reader.preferred_read_granularity())
            })
            .max()
            .unwrap_or(0);
        let info = ReaderInfo {
            path: std::path::PathBuf::from(format!("lvm://{}", lv_name)),
            size: total_size,
            kind: "LVM2 Logical Volume".to_string(),
        };

        Self {
            device_readers,
            info,
            extent_map,
            current_pos: 0,
            total_size,
            preferred_read_granularity,
        }
    }

    /// Find the extent that contains the given logical byte offset.
    /// Returns `(extent_index, offset_within_extent)`.
    fn locate(&self, logical_offset: u64) -> Option<(usize, u64)> {
        let idx = self
            .extent_map
            .partition_point(|ext| ext.logical_start <= logical_offset)
            .checked_sub(1)?;

        self.extent_map.get(idx).and_then(|ext| {
            let logical_end = ext.logical_start.checked_add(ext.length)?;
            if logical_offset < logical_end {
                Some((idx, logical_offset - ext.logical_start))
            } else {
                None
            }
        })
    }

    fn read_contiguous(
        &self,
        mut logical_offset: u64,
        mut buf: &mut [u8],
    ) -> std::io::Result<usize> {
        let mut total_read = 0usize;

        while !buf.is_empty() && logical_offset < self.total_size {
            let (ext_idx, offset_in_ext) = self.locate(logical_offset).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!(
                        "logical offset {} is not covered by any extent",
                        logical_offset
                    ),
                )
            })?;

            let ext = &self.extent_map[ext_idx];
            let available_in_ext = ext.length.saturating_sub(offset_in_ext);
            let remaining_in_volume = self.total_size.saturating_sub(logical_offset);
            let to_read = buf
                .len()
                .min(available_in_ext.min(remaining_in_volume) as usize);
            if to_read == 0 {
                break;
            }

            let physical_offset = ext.physical_offset + offset_in_ext;
            let device_reader = self.device_readers.get(ext.pv_index).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "extent references PV reader index {} but only {} readers are available",
                        ext.pv_index,
                        self.device_readers.len()
                    ),
                )
            })?;
            let mut reader = device_reader.lock().unwrap();
            reader.seek(SeekFrom::Start(physical_offset))?;
            reader.read_exact(&mut buf[..to_read])?;
            drop(reader);

            total_read += to_read;
            logical_offset += to_read as u64;
            let (_, rest) = buf.split_at_mut(to_read);
            buf = rest;
        }

        Ok(total_read)
    }

    /// Read at most `len` bytes starting at `logical_offset`.
    fn read_at(&self, logical_offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        if logical_offset >= self.total_size {
            return Ok(0);
        }

        self.read_contiguous(logical_offset, buf)
    }
}

impl Read for LvReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.read_at(self.current_pos, buf)?;
        self.current_pos += n as u64;
        Ok(n)
    }
}

impl Seek for LvReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.current_pos = match pos {
            SeekFrom::Start(o) => o,
            SeekFrom::End(o) => {
                let abs = self.total_size as i64 + o;
                if abs < 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "seek before start of volume",
                    ));
                }
                abs as u64
            }
            SeekFrom::Current(o) => {
                let abs = self.current_pos as i64 + o;
                if abs < 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "seek before start of volume",
                    ));
                }
                abs as u64
            }
        };
        Ok(self.current_pos)
    }
}

impl EvidenceReader for LvReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        self.preferred_read_granularity
    }
}

// EvidenceReader requires Send — LvReader is Send because:
// - device_reader: Box<dyn EvidenceReader> is Send (EvidenceReader: Send)
// - RefCell is !Sync but Send (single-threaded ownership transfer is fine)
// - All other fields are plain data (Send + Sync)

#[cfg(test)]
#[path = "../tests/unit/lv_reader.rs"]
mod tests;
