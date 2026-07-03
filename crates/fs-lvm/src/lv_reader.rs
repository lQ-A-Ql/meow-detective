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
}

// EvidenceReader requires Send — LvReader is Send because:
// - device_reader: Box<dyn EvidenceReader> is Send (EvidenceReader: Send)
// - RefCell is !Sync but Send (single-threaded ownership transfer is fine)
// - All other fields are plain data (Send + Sync)

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal fake reader for testing LvReader.
    struct FakeDevice {
        data: Vec<u8>,
        pos: u64,
    }

    impl FakeDevice {
        fn new(data: Vec<u8>) -> Self {
            Self { data, pos: 0 }
        }
    }

    impl Read for FakeDevice {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let start = self.pos as usize;
            let end = (start + buf.len()).min(self.data.len());
            let len = end.saturating_sub(start);
            buf[..len].copy_from_slice(&self.data[start..end]);
            self.pos += len as u64;
            Ok(len)
        }
    }

    impl Seek for FakeDevice {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.pos = match pos {
                SeekFrom::Start(o) => o,
                SeekFrom::End(o) => (self.data.len() as i64 + o).max(0) as u64,
                SeekFrom::Current(o) => (self.pos as i64 + o).max(0) as u64,
            };
            Ok(self.pos)
        }
    }

    impl EvidenceReader for FakeDevice {
        fn info(&self) -> &ReaderInfo {
            // Return a static reference — only used in tests
            unimplemented!("not needed for these tests")
        }
    }

    #[test]
    fn read_within_single_extent() {
        // PV data: 4 KB of zeros, then "HELLO WORLD DATA" at offset 4096
        let mut pv_data = vec![0u8; 4096];
        pv_data.extend_from_slice(b"HELLO WORLD DATA");

        let device = Box::new(FakeDevice::new(pv_data));
        let extent_map = vec![LvExtent {
            logical_start: 0,
            physical_offset: 4096,
            length: 17,
            pv_index: 0,
        }];

        let lv = LvReader::new(device, "test_lv".into(), 17, extent_map);
        let reader = std::sync::Mutex::new(Box::new(lv) as Box<dyn EvidenceReader>);
        let mut lv_ref = reader.lock().unwrap();

        let mut buf = [0u8; 5];
        lv_ref.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"HELLO");
    }

    #[test]
    fn read_across_extents_uses_extent_pv_index() {
        let mut pv0_data = vec![0u8; 32];
        pv0_data[8..12].copy_from_slice(b"PV00");
        let mut pv1_data = vec![0u8; 32];
        pv1_data[8..12].copy_from_slice(b"PV11");

        let pv0: Box<dyn EvidenceReader> = Box::new(FakeDevice::new(pv0_data));
        let pv1: Box<dyn EvidenceReader> = Box::new(FakeDevice::new(pv1_data));
        let device_readers = vec![
            std::sync::Arc::new(std::sync::Mutex::new(pv0)),
            std::sync::Arc::new(std::sync::Mutex::new(pv1)),
        ];
        let extent_map = vec![
            LvExtent {
                logical_start: 0,
                physical_offset: 8,
                length: 4,
                pv_index: 0,
            },
            LvExtent {
                logical_start: 4,
                physical_offset: 8,
                length: 4,
                pv_index: 1,
            },
        ];

        let mut lv = LvReader::new_shared(device_readers, "striped_lv".into(), 8, extent_map);

        let mut buf = [0u8; 8];
        lv.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"PV00PV11");
    }

    #[test]
    fn plain_read_fills_across_extent_boundaries() {
        let mut pv0_data = vec![0u8; 32];
        pv0_data[8..12].copy_from_slice(b"PV00");
        let mut pv1_data = vec![0u8; 32];
        pv1_data[8..12].copy_from_slice(b"PV11");

        let device_readers = vec![
            std::sync::Arc::new(std::sync::Mutex::new(
                Box::new(FakeDevice::new(pv0_data)) as Box<dyn EvidenceReader>
            )),
            std::sync::Arc::new(std::sync::Mutex::new(
                Box::new(FakeDevice::new(pv1_data)) as Box<dyn EvidenceReader>
            )),
        ];
        let extent_map = vec![
            LvExtent {
                logical_start: 0,
                physical_offset: 8,
                length: 4,
                pv_index: 0,
            },
            LvExtent {
                logical_start: 4,
                physical_offset: 8,
                length: 4,
                pv_index: 1,
            },
        ];

        let mut lv = LvReader::new_shared(device_readers, "striped_lv".into(), 8, extent_map);
        let mut buf = [0u8; 8];
        let n = lv.read(&mut buf).unwrap();

        assert_eq!(n, 8);
        assert_eq!(&buf, b"PV00PV11");
    }

    #[test]
    fn seek_and_read() {
        let mut pv_data = vec![0u8; 2048];
        pv_data.extend_from_slice(b"ABCDEFGHIJ");

        let device = Box::new(FakeDevice::new(pv_data));
        let extent_map = vec![LvExtent {
            logical_start: 0,
            physical_offset: 2048,
            length: 10,
            pv_index: 0,
        }];

        let lv = LvReader::new(device, "test_lv".into(), 10, extent_map);
        let reader = std::sync::Mutex::new(Box::new(lv) as Box<dyn EvidenceReader>);
        let mut lv_ref = reader.lock().unwrap();

        // Seek to offset 3, read 2 bytes
        lv_ref.seek(SeekFrom::Start(3)).unwrap();
        let mut buf = [0u8; 2];
        lv_ref.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"DE");
    }

    #[test]
    fn read_past_end_returns_zero() {
        let mut pv_data = vec![0u8; 1024];
        pv_data.extend_from_slice(b"DATA");

        let device = Box::new(FakeDevice::new(pv_data));
        let extent_map = vec![LvExtent {
            logical_start: 0,
            physical_offset: 1024,
            length: 4,
            pv_index: 0,
        }];

        let lv = LvReader::new(device, "small_lv".into(), 4, extent_map);
        let reader = std::sync::Mutex::new(Box::new(lv) as Box<dyn EvidenceReader>);
        let mut lv_ref = reader.lock().unwrap();

        lv_ref.seek(SeekFrom::Start(4)).unwrap();
        let mut buf = [0u8; 10];
        let n = lv_ref.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }
}
