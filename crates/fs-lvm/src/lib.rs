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

/// Shared device reader type used across multiple LV readers.
type SharedReader = std::sync::Arc<std::sync::Mutex<Box<dyn EvidenceReader>>>;

/// Parsed PV label and reader paired with the partition offset supplied by the caller.
struct DiscoveredPv {
    reader: SharedReader,
    label: LvmLabel,
    pv_offset: u64,
}

fn lvm_uuid_matches(label_uuid: &str, metadata_uuid: &str) -> bool {
    let label = normalize_lvm_uuid(label_uuid);
    let metadata = normalize_lvm_uuid(metadata_uuid);
    !label.is_empty() && label == metadata
}

fn normalize_lvm_uuid(uuid: &str) -> String {
    uuid.trim().chars().filter(|ch| *ch != '-').collect()
}

/// An opened LVM2 volume group with parsed metadata and ready-to-open logical
/// volumes.
pub struct LvmPool {
    volume_group: VolumeGroup,
    /// Shared device readers in the same order as `volume_group.physical_volumes`.
    device_readers: Vec<SharedReader>,
    /// Stable mapping from metadata PV name to the caller-supplied PV start offset.
    pv_start_offsets: Vec<(String, u64)>,
    /// Stable mapping from metadata PV name to absolute data-area start offset.
    pv_data_offsets: Vec<(String, u64)>,
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

        // Phase 1: Parse PV label from EACH PV, store reader+label+offset
        let mut pv_entries: Vec<DiscoveredPv> = Vec::with_capacity(readers.len());
        for (reader, pv_off) in readers.into_iter().zip(pv_offsets) {
            let cell = std::sync::Mutex::new(reader);
            let pv_label = {
                let mut r = cell.lock().unwrap();
                label::parse_pv_label(&mut *r, pv_off)?
            };
            pv_entries.push(DiscoveredPv {
                reader: Arc::new(cell),
                label: pv_label,
                pv_offset: pv_off,
            });
        }

        // Phase 2: Parse VG metadata from the first PV
        let first_entry = &pv_entries[0];
        let vg = {
            let mda = first_entry.label.metadata_areas.first().ok_or_else(|| {
                LvmError::MetadataParseError {
                    line: 0,
                    message: "no metadata area found on PV".to_string(),
                }
            })?;
            let mut r = first_entry.reader.lock().unwrap();
            metadata::parse_metadata(&mut *r, mda, first_entry.pv_offset)?
        };

        // Phase 3: Match PV UUIDs from metadata to reader entries,
        // building absolute (reader-start) data area offsets for each PV.
        let mut device_readers = Vec::with_capacity(vg.physical_volumes.len());
        let mut pv_start_offsets = Vec::with_capacity(vg.physical_volumes.len());
        let mut pv_data_offsets = Vec::with_capacity(vg.physical_volumes.len());
        for pv_meta in &vg.physical_volumes {
            // Find matching reader by PV UUID
            let matched = pv_entries
                .iter()
                .find(|entry| lvm_uuid_matches(&entry.label.pv_uuid, &pv_meta.uuid))
                .ok_or_else(|| LvmError::MissingPhysicalVolumeReader {
                    pv_name: pv_meta.name.clone(),
                    pv_uuid: pv_meta.uuid.clone(),
                })?;
            let data_area =
                matched
                    .label
                    .data_areas
                    .first()
                    .ok_or_else(|| LvmError::MetadataParseError {
                        line: 0,
                        message: format!("PV '{}' has no data area", pv_meta.name),
                    })?;
            let data_offset = matched.pv_offset + data_area.offset;

            device_readers.push(matched.reader.clone());
            pv_start_offsets.push((pv_meta.name.clone(), matched.pv_offset));
            pv_data_offsets.push((pv_meta.name.clone(), data_offset));
        }

        let logical_volumes = vg.logical_volumes.clone();

        Ok(LvmPool {
            volume_group: vg,
            device_readers,
            pv_start_offsets,
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
    /// Uses Arc-shared device readers so multiple LVs can be opened
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

        Ok(LvReader::new_shared(
            self.device_readers.clone(),
            lv.name.clone(),
            lv.size_bytes,
            extent_map,
        ))
    }

    /// Access the parsed volume group metadata.
    pub fn volume_group(&self) -> &VgMeta {
        &self.volume_group
    }

    /// Return stable `(pv_name, pv_start_byte)` mappings in VG metadata PV order.
    ///
    /// These are the offsets that callers must pass back into [`LvmPool::discover`].
    /// Segment mapping uses the separate internal data-area offsets.
    pub fn physical_volume_offsets(&self) -> &[(String, u64)] {
        &self.pv_start_offsets
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

    const SYNTHETIC_PV_SIZE: u64 = 2_097_152;
    const SYNTHETIC_DATA_AREA_START: u64 = 2560;
    const PV0_UUID: &str = "00000000000000000000000000000000";
    const PV1_UUID: &str = "11111111111111111111111111111111";

    fn build_synthetic_multi_pv_disks() -> (Vec<u8>, Vec<u8>) {
        let metadata_text = format!(
            r#"test_vg {{
    id = "vg-multi-pv-1234"
    seqno = 2
    extent_size = 1

    physical_volumes {{
        pv0 {{
            id = "{}"
            device = "/dev/sda1"
            pe_start = 0
            pe_count = 16
        }}
        pv1 {{
            id = "{}"
            device = "/dev/sdb1"
            pe_start = 0
            pe_count = 16
        }}
    }}

    logical_volumes {{
        stripe {{
            id = "lv-striped-uuid"
            segment_count = 1
            segment1 {{
                start_extent = 0
                extent_count = 2
                type = "striped"
                stripe_count = 2
                stripes = ["pv0", 0, "pv1", 0]
            }}
        }}
    }}
}}
"#,
            PV0_UUID, PV1_UUID
        );

        let mut pv0 = vec![0u8; SYNTHETIC_PV_SIZE as usize];
        let mut pv1 = vec![0u8; SYNTHETIC_PV_SIZE as usize];
        write_synthetic_pv_label(&mut pv0, PV0_UUID);
        write_synthetic_pv_label(&mut pv1, PV1_UUID);
        write_synthetic_metadata(&mut pv0, &metadata_text);
        write_synthetic_metadata(&mut pv1, &metadata_text);
        (pv0, pv1)
    }

    fn write_synthetic_pv_label(disk: &mut [u8], pv_uuid: &str) {
        let pv_size = disk.len() as u64;
        let sec = &mut disk[512..1024];
        sec[0..8].copy_from_slice(b"LABELONE");
        sec[8..16].copy_from_slice(&1u64.to_le_bytes());
        sec[20..24].copy_from_slice(&32u32.to_le_bytes());
        sec[24..32].copy_from_slice(b"LVM2 001");

        let uuid_padded = format!("{:32}", pv_uuid);
        sec[32..64].copy_from_slice(&uuid_padded.as_bytes()[..32]);
        sec[64..72].copy_from_slice(&pv_size.to_le_bytes());
        sec[72..80].copy_from_slice(&SYNTHETIC_DATA_AREA_START.to_le_bytes());
        sec[80..88].copy_from_slice(&(pv_size - SYNTHETIC_DATA_AREA_START).to_le_bytes());
        sec[104..112].copy_from_slice(&1024u64.to_le_bytes());
        sec[112..120].copy_from_slice(&(4 * 512u64).to_le_bytes());

        let crc = crc::lvm_crc32(&sec[20..512]);
        sec[16..20].copy_from_slice(&crc.to_le_bytes());
    }

    fn write_synthetic_metadata(disk: &mut [u8], metadata_text: &str) {
        let text_bytes = metadata_text.as_bytes();
        let text_offset = 1536usize;
        let text_end = text_offset + text_bytes.len();
        assert!(text_end <= disk.len());

        {
            let mda = &mut disk[1024..1536];
            mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
            mda[20..24].copy_from_slice(&1u32.to_le_bytes());
            mda[24..32].copy_from_slice(&1024u64.to_le_bytes());
            mda[32..40].copy_from_slice(&1536u64.to_le_bytes());

            let rl_base: usize = 40;
            mda[rl_base..rl_base + 8].copy_from_slice(&512u64.to_le_bytes());
        }

        disk[text_offset..text_end].copy_from_slice(text_bytes);

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
        assert_eq!(pool.physical_volume_offsets(), &[("pv0".to_string(), 0)]);
    }

    #[test]
    fn discover_matches_label_uuid_to_dashed_metadata_uuid() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = r#"test_vg {
    id = "vg-dashed-pv-uuid"
    seqno = 2
    extent_size = 8192

    physical_volumes {
        pv0 {
            id = "abcdef12-3456-7890-abcd-ef1234567890"
            device = "/dev/sda1"
            pe_start = 0
            pe_count = 10
        }
    }

    logical_volumes {
        root {
            id = "lv-root-uuid-1234-5678"
            segment_count = 1
            segment1 {
                start_extent = 0
                extent_count = 5
                type = "striped"
                stripe_count = 1
                stripes = ["pv0", 0]
            }
        }
    }
}
"#;
        write_synthetic_metadata(&mut disk, metadata_text);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();

        assert_eq!(pool.volume_group().physical_volumes[0].name, "pv0");
        assert_eq!(
            pool.volume_group().physical_volumes[0].uuid,
            "abcdef12-3456-7890-abcd-ef1234567890"
        );
        assert_eq!(pool.physical_volume_offsets(), &[("pv0".to_string(), 0)]);
        assert_eq!(pool.list_volumes().len(), 1);
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

    #[test]
    fn discover_binds_readers_in_metadata_pv_order() {
        let (mut pv0, mut pv1) = build_synthetic_multi_pv_disks();
        let first_extent = SYNTHETIC_DATA_AREA_START as usize;
        pv0[first_extent..first_extent + 512].fill(b'A');
        pv1[first_extent..first_extent + 512].fill(b'B');

        let pv1_reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(pv1));
        let pv0_reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(pv0));
        let pool = LvmPool::discover(vec![pv1_reader, pv0_reader], vec![0, 0]).unwrap();

        assert_eq!(
            pool.physical_volume_offsets(),
            &[("pv0".to_string(), 0), ("pv1".to_string(), 0)]
        );

        let mut lv = pool.open_volume(0).unwrap();
        let mut buf = vec![0u8; 1024];
        lv.read_exact(&mut buf).unwrap();

        assert!(buf[..512].iter().all(|b| *b == b'A'));
        assert!(buf[512..].iter().all(|b| *b == b'B'));
    }

    #[test]
    fn discover_missing_metadata_pv_reader_fails_closed() {
        let (pv0, _pv1) = build_synthetic_multi_pv_disks();
        let pv0_reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(pv0));

        let err = match LvmPool::discover(vec![pv0_reader], vec![0]) {
            Ok(_) => panic!("expected missing PV reader error"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            LvmError::MissingPhysicalVolumeReader {
                pv_name,
                pv_uuid
            } if pv_name == "pv1" && pv_uuid == PV1_UUID
        ));
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
