//! Read-only LVM2 (Logical Volume Manager) parser for disk forensics.
//!
//! This crate discovers logical volumes on LVM2 physical volumes and exposes
//! each logical volume as a virtual block device implementing
//! [`evidence_core::EvidenceReader`], so existing filesystem readers (ext4,
//! xfs, btrfs) can mount logical volumes without modification.
//!
//! # Architecture
//!
//! ```text
//! Disk Image → MBR/GPT → PV Label → VG Metadata → LV Extent Map → LvReader
//!                                                                    ↓
//!                                              Ext4Reader::open(lv_reader, 0)
//! ```
//!
//! # Supported configurations
//!
//! - Single-PV volume groups with linear logical volumes (Phase 1)
//! - Multi-PV and striped (RAID 0) configurations (Phase 2, planned)
//!
//! # Usage
//!
//! ```ignore
//! use fs_lvm;
//! use evidence_core::EvidenceReader;
//!
//! // Probe whether a partition is an LVM physical volume
//! if fs_lvm::probe_lvm(&mut reader, partition_offset)? {
//!     // Open the LVM pool and discover logical volumes
//!     let pool = fs_lvm::LvmPool::discover(
//!         vec![reader],
//!         vec![partition_offset],
//!     )?;
//!
//!     for lv in pool.list_volumes() {
//!         println!("LV: {} ({:.1} MB)", lv.name, lv.size_bytes as f64 / 1_048_576.0);
//!     }
//!
//!     // Open a specific logical volume as a virtual block device
//!     let lv_reader = pool.open_volume(0)?;
//!     // Feed it to a filesystem reader (offset = 0, LV is a clean block device)
//!     let ext4 = fs_ext4::Ext4Reader::open(Box::new(lv_reader), 0)?;
//! }
//! ```

pub mod crc;
pub mod error;
pub mod label;
pub mod lv_reader;
pub mod metadata;
pub mod segment;

use std::sync::Arc;

use evidence_core::EvidenceReader;

use crate::error::Result;

// --- Re-exports ---
pub use crate::error::LvmError;
pub use crate::label::LvmLabel;
pub use crate::lv_reader::LvReader;
pub use crate::metadata::{LvMeta, PvMeta, SegmentMeta, SegmentType, VolumeGroup};
pub use crate::segment::LvExtent;

// Use internally (not re-exported)
use crate::metadata::VolumeGroup as VgMeta;

// --- Public API ---

/// Probe whether the data at `offset` in `reader` is an LVM2 physical volume.
///
/// Reads sector 1 (offset + 512), checks for `"LABELONE"` and `"LVM2 001"`
/// magic bytes, and verifies the label CRC-32.
pub fn probe_lvm(
    reader: &mut (impl std::io::Read + std::io::Seek),
    pv_offset: u64,
) -> Result<bool> {
    match label::parse_pv_label(reader, pv_offset) {
        Ok(_) => Ok(true),
        Err(LvmError::NotLvm) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Represents a discovered logical volume.
#[derive(Debug, Clone)]
pub struct LvInfo {
    pub name: String,
    pub uuid: String,
    pub size_bytes: u64,
}

/// An opened LVM2 volume group with parsed metadata and ready-to-open logical
/// volumes.
pub struct LvmPool {
    volume_group: VolumeGroup,
    /// Shared device readers (Rc allows multiple LvReaders to share one PV reader).
    device_readers: Vec<std::sync::Arc<std::sync::Mutex<Box<dyn EvidenceReader>>>>,
    pv_data_offsets: Vec<(String, u64)>, // (pv_name, data_area_start_byte)
    logical_volumes: Vec<LvMeta>,
}

impl LvmPool {
    /// Scan one or more physical volumes and discover the volume group and its
    /// logical volumes.
    ///
    /// # Arguments
    ///
    /// * `readers` — One `EvidenceReader` per physical volume. For a
    ///   single-disk setup this is a single-element vector.
    /// * `pv_offsets` — Byte offset of each PV's start in its corresponding
    ///   reader (typically the MBR/GPT partition's LBA start × 512).
    pub fn discover(readers: Vec<Box<dyn EvidenceReader>>, pv_offsets: Vec<u64>) -> Result<Self> {
        if readers.is_empty() || readers.len() != pv_offsets.len() {
            return Err(crate::error::LvmError::MetadataParseError {
                line: 0,
                message: "readers and pv_offsets must be non-empty and same length".to_string(),
            });
        }

        // Phase 1: Parse PV label from the first PV
        let mut pv_reader = readers.into_iter();
        let mut pv_offset_iter = pv_offsets.into_iter();

        let first_reader = pv_reader.next().unwrap();
        let first_offset = pv_offset_iter.next().unwrap();

        let temp_reader = std::sync::Mutex::new(first_reader);

        // Phase 1: Parse PV label
        let pv_label = {
            let mut r = temp_reader.lock().unwrap();
            label::parse_pv_label(&mut *r, first_offset)?
        };

        // Phase 2: Parse metadata (separate scope so borrow is released)
        let vg =
            {
                let mda = pv_label.metadata_areas.first().ok_or_else(|| {
                    LvmError::MetadataParseError {
                        line: 0,
                        message: "no metadata area found on PV".to_string(),
                    }
                })?;
                let mut r = temp_reader.lock().unwrap();
                metadata::parse_metadata(&mut *r, mda, first_offset)?
            };

        // Phase 3: Build PV data offset map
        let first_data_area =
            pv_label
                .data_areas
                .first()
                .ok_or_else(|| LvmError::MetadataParseError {
                    line: 0,
                    message: "no data area found on PV".to_string(),
                })?;

        let mut pv_data_offsets: Vec<(String, u64)> = Vec::new();
        for pv_meta in &vg.physical_volumes {
            // Physical offset = PV start in reader + data area start within PV
            pv_data_offsets.push((pv_meta.name.clone(), first_offset + first_data_area.offset));
        }

        let logical_volumes = vg.logical_volumes.clone();

        Ok(LvmPool {
            volume_group: vg,
            device_readers: vec![Arc::new(temp_reader)],
            pv_data_offsets,
            logical_volumes,
        })
    }

    /// List all logical volumes discovered in this volume group.
    pub fn list_volumes(&self) -> Vec<LvInfo> {
        self.logical_volumes
            .iter()
            .map(|lv| LvInfo {
                name: lv.name.clone(),
                uuid: lv.uuid.clone(),
                size_bytes: lv.size_bytes,
            })
            .collect()
    }

    /// Open a logical volume by index, returning a virtual block device
    /// that implements [`EvidenceReader`].
    ///
    /// Uses `Rc`-shared device reader so multiple LVs can be opened
    /// concurrently from a single pool without consuming it.
    ///
    /// The returned reader can be passed to filesystem readers like
    /// `Ext4Reader::open()`, `XfsReader::open()`, etc. with offset `0`
    /// since a logical volume presents as a clean block device.
    pub fn open_volume(&self, index: usize) -> Result<LvReader> {
        if index >= self.logical_volumes.len() {
            return Err(crate::error::LvmError::LvIndexOutOfRange {
                index,
                count: self.logical_volumes.len(),
            });
        }

        let lv = &self.logical_volumes[index];
        let extent_map = segment::build_extent_map(&self.volume_group, lv, &self.pv_data_offsets)?;

        let shared_reader = self.device_readers[0].clone();

        Ok(LvReader::new_shared(shared_reader, lv.name.clone(), lv.size_bytes, extent_map))
    }

    /// Access the parsed volume group metadata.
    pub fn volume_group(&self) -> &VgMeta {
        &self.volume_group
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, SeekFrom};

    /// Build a minimal synthetic LVM2 disk image with one PV, one VG, one LV.
    ///
    /// Layout:
    /// ```text
    /// Sector 0:    empty (zero-filled)
    /// Sector 1:    PV label + PV header
    /// Sector 2-4:  metadata area (ASCII text)
    /// Sector 5+:   data area (from LBA 5 = offset 2560)
    /// ```
    fn build_synthetic_lvm_disk() -> Vec<u8> {
        let pv_uuid = "abcdef1234567890abcdef1234567890";
        let pv_size = 2_097_152u64; // 2 MB
        let mut disk = vec![0u8; pv_size as usize];

        // --- Sector 1 (offset 512): PV label + header ---
        {
            let (label_sec, _rest) = disk.split_at_mut(1024);
            let sec = &mut label_sec[512..1024];
            sec[0..8].copy_from_slice(b"LABELONE");
            sec[8..16].copy_from_slice(&1u64.to_le_bytes());
            sec[20..24].copy_from_slice(&32u32.to_le_bytes());
            sec[24..32].copy_from_slice(b"LVM2 001");

            let uuid_padded = format!("{:32}", pv_uuid);
            sec[32..64].copy_from_slice(uuid_padded.as_bytes());
            sec[64..72].copy_from_slice(&pv_size.to_le_bytes());
            sec[72..80].copy_from_slice(&2560u64.to_le_bytes());
            sec[80..88].copy_from_slice(&(pv_size - 2560).to_le_bytes());
            sec[104..112].copy_from_slice(&1024u64.to_le_bytes()); // MDA at sector 2
            sec[112..120].copy_from_slice(&(4 * 512u64).to_le_bytes());

            let crc = crc::lvm_crc32(&sec[20..512]);
            sec[16..20].copy_from_slice(&crc.to_le_bytes());
        }

        // --- Build metadata text first (needed for MDA) ---
        let metadata_text = format!(
            r#"test_vg {{
    id = "vg-1234-5678-90ab-cdef"
    seqno = 1
    extent_size = 8192

    physical_volumes {{
        pv0 {{
            id = "{}"
            device = "/dev/sda1"
            pe_start = 0
            pe_count = 10
        }}
    }}

    logical_volumes {{
        root {{
            id = "lv-root-uuid-1234-5678"
            segment_count = 1
            segment1 {{
                start_extent = 0
                extent_count = 5
                type = "striped"
                stripe_count = 1
                stripes = ["pv0", 0]
            }}
        }}
    }}
}}
"#,
            pv_uuid
        );
        let text_bytes = metadata_text.as_bytes();

        // --- Sector 2 (offset 1024): MDA header ---
        {
            let mda = &mut disk[1024..1536];
            mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
            mda[20..24].copy_from_slice(&1u32.to_le_bytes());
            mda[24..32].copy_from_slice(&1024u64.to_le_bytes());
            mda[32..40].copy_from_slice(&1536u64.to_le_bytes());

            let rl_base: usize = 40;
            mda[rl_base..rl_base + 8].copy_from_slice(&512u64.to_le_bytes());
            // size and checksum filled below
        }

        // --- Sector 3 (offset 1536): metadata text ---
        let text_offset: usize = 1536;
        let text_end = text_offset + text_bytes.len();
        if text_end <= disk.len() {
            disk[text_offset..text_end].copy_from_slice(text_bytes);
        }

        // Now update MDA with computed size/checksums
        let text_size = text_bytes.len() as u64;
        let text_crc = crc::lvm_crc32(text_bytes);
        {
            let mda = &mut disk[1024..1536];
            let rl_base: usize = 40;
            mda[rl_base + 8..rl_base + 16].copy_from_slice(&text_size.to_le_bytes());
            mda[rl_base + 16..rl_base + 20].copy_from_slice(&text_crc.to_le_bytes());
            let mda_crc = crc::lvm_crc32(&mda[4..512]);
            mda[0..4].copy_from_slice(&mda_crc.to_le_bytes());
        }

        disk
    }

    #[test]
    fn probe_detects_lvm() {
        let disk = build_synthetic_lvm_disk();
        let mut reader = Cursor::new(&disk);
        assert!(probe_lvm(&mut reader, 0).unwrap());
    }

    #[test]
    fn probe_rejects_non_lvm() {
        let disk = vec![0u8; 2048]; // just zeroes
        let mut reader = Cursor::new(&disk);
        assert!(!probe_lvm(&mut reader, 0).unwrap());
    }

    #[test]
    fn discover_parses_volume_group() {
        let disk = build_synthetic_lvm_disk();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();

        let vg = pool.volume_group();
        assert_eq!(vg.name, "test_vg");
        assert_eq!(vg.extent_size, 8192);
        assert_eq!(vg.seqno, 1);
        assert_eq!(vg.physical_volumes.len(), 1);
        assert_eq!(vg.physical_volumes[0].name, "pv0");

        let vols = pool.list_volumes();
        assert_eq!(vols.len(), 1);
        assert_eq!(vols[0].name, "root");
        assert_eq!(vols[0].size_bytes, 5 * 8192 * 512); // 5 extents
    }

    #[test]
    fn open_volume_reads_data() {
        let mut disk = build_synthetic_lvm_disk();

        // Write "FORENSIC TEST DATA" at the data area offset (sector 5 = 2560)
        let test_data = b"FORENSIC TEST DATA AT LV OFFSET 0";
        let data_area_start = 2560usize;
        disk[data_area_start..data_area_start + test_data.len()].copy_from_slice(test_data);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();
        let mut lv = pool.open_volume(0).unwrap();

        let mut buf = vec![0u8; test_data.len()];
        lv.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, test_data);
    }

    // --- Test helpers ---
    use evidence_core::ReaderInfo;
    use std::io::{Read as IoRead, Seek as IoSeek};

    struct FakeDiskReader {
        data: Vec<u8>,
        pos: u64,
    }

    impl FakeDiskReader {
        fn new(data: Vec<u8>) -> Self {
            Self { data, pos: 0 }
        }
    }

    impl IoRead for FakeDiskReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let start = self.pos as usize;
            let end = (start + buf.len()).min(self.data.len());
            let len = end.saturating_sub(start);
            buf[..len].copy_from_slice(&self.data[start..end]);
            self.pos += len as u64;
            Ok(len)
        }
    }

    impl IoSeek for FakeDiskReader {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.pos = match pos {
                SeekFrom::Start(o) => o,
                SeekFrom::End(o) => (self.data.len() as i64 + o).max(0) as u64,
                SeekFrom::Current(o) => (self.pos as i64 + o).max(0) as u64,
            };
            Ok(self.pos)
        }
    }

    impl EvidenceReader for FakeDiskReader {
        fn info(&self) -> &ReaderInfo {
            unimplemented!()
        }
    }
}
