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
mod pool_thin;
pub mod segment;
pub mod thin;
pub mod thin_reader;

use std::sync::Arc;

use evidence_core::EvidenceReader;

use crate::error::Result;

// --- Re-exports ---
pub use crate::error::LvmError;
pub use crate::label::LvmLabel;
pub use crate::lv_reader::LvReader;
pub use crate::metadata::{LvMeta, PvMeta, SegmentArea, SegmentMeta, SegmentType, VolumeGroup};
pub use crate::segment::LvExtent;
pub use crate::thin_reader::ThinLvReader;

// Use internally (not re-exported)
use crate::metadata::VolumeGroup as VgMeta;

// --- Public API ---

/// Probe whether the data at `offset` in `reader` is an LVM2 physical volume.
///
/// Reads sector 1 (offset + 512), checks for `"LABELONE"` and `"LVM2 001"`
/// magic bytes, and verifies the label CRC-32.
pub fn probe_lvm<R>(reader: &mut R, pv_offset: u64) -> Result<bool>
where
    R: std::io::Read + std::io::Seek + ?Sized,
{
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
    pub role: String,
    pub status: Vec<String>,
    pub visible: bool,
    pub directly_mappable: bool,
    pub unsupported_reason: Option<String>,
}

/// Shared device reader type used across multiple LV readers.
type SharedReader = std::sync::Arc<std::sync::Mutex<Box<dyn EvidenceReader>>>;

/// Parsed PV label and reader paired with the partition offset supplied by the caller.
struct DiscoveredPv {
    reader: SharedReader,
    label: LvmLabel,
    pv_offset: u64,
}

#[derive(Debug, Clone)]
struct ResolvedPvMapping {
    name: String,
    start_offset: u64,
    data_offset: u64,
    data_size: u64,
    pv_size: u64,
}

fn lvm_uuid_matches(label_uuid: &str, metadata_uuid: &str) -> bool {
    let label = normalize_lvm_uuid(label_uuid);
    let metadata = normalize_lvm_uuid(metadata_uuid);
    !label.is_empty() && label == metadata
}

fn normalize_lvm_uuid(uuid: &str) -> String {
    uuid.trim()
        .chars()
        .filter(|ch| *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
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

        // Phase 2: On each supplied PV, consider all metadata areas and keep
        // the highest valid seqno. This mirrors LVM2's redundant metadata copy
        // selection while staying tolerant of stale or corrupt copies.
        let mut vg: Option<VolumeGroup> = None;
        let mut first_fatal_metadata_error: Option<LvmError> = None;
        for entry in &pv_entries {
            if entry.label.metadata_areas.is_empty() {
                continue;
            }
            let mut r = entry.reader.lock().unwrap();
            let candidate = match metadata::parse_metadata_from_regions(
                &mut *r,
                &entry.label.metadata_areas,
                entry.pv_offset,
            ) {
                Ok(candidate) => candidate,
                Err(LvmError::MetadataParseError { .. })
                | Err(LvmError::MdaCrcMismatch { .. })
                | Err(LvmError::MetadataCrcMismatch { .. }) => continue,
                Err(err @ LvmError::FatalMetadataParseError { .. }) => {
                    if first_fatal_metadata_error.is_none() {
                        first_fatal_metadata_error = Some(err);
                    }
                    continue;
                }
                Err(err) => return Err(err),
            };
            if vg
                .as_ref()
                .is_none_or(|current| candidate.seqno > current.seqno)
            {
                vg = Some(candidate);
            }
        }
        let vg = match vg {
            Some(vg) => vg,
            None => {
                if let Some(err) = first_fatal_metadata_error {
                    return Err(err);
                }
                return Err(LvmError::MetadataParseError {
                    line: 0,
                    message: "no valid metadata copy found on supplied physical volumes"
                        .to_string(),
                });
            }
        };

        // Phase 3: Match PV UUIDs from metadata to reader entries,
        // building absolute (reader-start) data area offsets for each PV.
        let mut device_readers = Vec::with_capacity(vg.physical_volumes.len());
        let mut pv_mappings = Vec::with_capacity(vg.physical_volumes.len());
        for pv_meta in &vg.physical_volumes {
            // Find matching reader by PV UUID
            let matched = pv_entries
                .iter()
                .find(|entry| lvm_uuid_matches(&entry.label.pv_uuid, &pv_meta.uuid))
                .ok_or_else(|| LvmError::MissingPhysicalVolumeReader {
                    pv_name: pv_meta.name.clone(),
                    pv_uuid: pv_meta.uuid.clone(),
                })?;
            let mapping = resolve_pv_mapping(pv_meta, matched)?;

            device_readers.push(matched.reader.clone());
            pv_mappings.push(mapping);
        }

        let logical_volumes = vg.logical_volumes.clone();
        let pv_start_offsets: Vec<(String, u64)> = pv_mappings
            .iter()
            .map(|mapping| (mapping.name.clone(), mapping.start_offset))
            .collect();
        let pv_data_offsets: Vec<(String, u64)> = pv_mappings
            .iter()
            .map(|mapping| (mapping.name.clone(), mapping.data_offset))
            .collect();
        let pv_data_bounds = pv_mappings
            .iter()
            .map(|mapping| {
                (
                    mapping.name.clone(),
                    mapping.data_offset,
                    mapping.data_size,
                    mapping.pv_size,
                )
            })
            .collect::<Vec<_>>();
        for lv in &logical_volumes {
            if lv.is_directly_mappable() {
                let extent_map = segment::build_extent_map(&vg, lv, &pv_data_offsets)?;
                validate_extent_map(lv, &extent_map, &pv_data_bounds)?;
            }
        }

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
        self.logical_volumes.iter().map(lv_info_from_meta).collect()
    }

    /// List only logical volumes that should be exposed as ordinary block
    /// devices to filesystem probes.
    pub fn list_direct_volumes(&self) -> Vec<(usize, LvInfo)> {
        self.logical_volumes
            .iter()
            .enumerate()
            .filter(|(_, lv)| lv.is_directly_mappable())
            .map(|(index, lv)| (index, lv_info_from_meta(lv)))
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
        self.open_mapped_volume(index)
    }

    fn open_mapped_volume(&self, index: usize) -> Result<LvReader> {
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

    /// Return stable `(pv_name, pv_data_start_byte)` mappings in VG metadata PV order.
    pub fn physical_volume_data_offsets(&self) -> &[(String, u64)] {
        &self.pv_data_offsets
    }

    /// Return stable `(pv_name, pv_start_byte)` mappings in VG metadata PV order.
    ///
    /// These are the offsets that callers must pass back into [`LvmPool::discover`].
    /// Segment mapping uses the separate internal data-area offsets.
    pub fn physical_volume_offsets(&self) -> &[(String, u64)] {
        &self.pv_start_offsets
    }
}

fn resolve_pv_mapping(pv_meta: &PvMeta, matched: &DiscoveredPv) -> Result<ResolvedPvMapping> {
    let pe_start_bytes =
        pv_meta
            .pe_start
            .checked_mul(512)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: format!("PV '{}' pe_start overflows bytes", pv_meta.name),
            })?;
    let label_data_area =
        matched
            .label
            .data_areas
            .first()
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: format!(
                    "PV '{}' ({}) has no data area descriptor",
                    pv_meta.name, pv_meta.uuid
                ),
            })?;
    if label_data_area.offset != pe_start_bytes {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "PV '{}' ({}) data area mismatch: label offset {} but metadata pe_start {} sectors = {} bytes",
                pv_meta.name, pv_meta.uuid, label_data_area.offset, pv_meta.pe_start, pe_start_bytes
            ),
        });
    }
    let data_offset = matched
        .pv_offset
        .checked_add(label_data_area.offset)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("PV '{}' data offset overflows bytes", pv_meta.name),
        })?;
    let data_size = if label_data_area.size == 0 {
        matched
            .label
            .pv_size
            .checked_sub(label_data_area.offset)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: format!(
                    "PV '{}' data area offset {} exceeds PV size {}",
                    pv_meta.name, label_data_area.offset, matched.label.pv_size
                ),
            })?
    } else {
        label_data_area.size
    };
    if label_data_area.offset.saturating_add(data_size) > matched.label.pv_size {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "PV '{}' data area range offset={} size={} exceeds PV size {}",
                pv_meta.name, label_data_area.offset, data_size, matched.label.pv_size
            ),
        });
    }

    Ok(ResolvedPvMapping {
        name: pv_meta.name.clone(),
        start_offset: matched.pv_offset,
        data_offset,
        data_size,
        pv_size: matched.label.pv_size,
    })
}

fn validate_extent_map(
    lv: &LvMeta,
    extent_map: &[LvExtent],
    pv_bounds: &[(String, u64, u64, u64)],
) -> Result<()> {
    if lv.size_bytes == 0 {
        return Ok(());
    }
    if extent_map.is_empty() {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("logical volume '{}' has no extent mappings", lv.name),
        });
    }

    let mut expected = 0u64;
    for extent in extent_map {
        if extent.logical_start != expected {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!(
                    "logical volume '{}' extent map has gap/overlap: expected logical offset {} but found {}",
                    lv.name, expected, extent.logical_start
                ),
            });
        }
        expected =
            expected
                .checked_add(extent.length)
                .ok_or_else(|| LvmError::MetadataParseError {
                    line: 0,
                    message: format!("logical volume '{}' extent map overflows", lv.name),
                })?;

        let Some((pv_name, data_start, data_size, pv_size)) = pv_bounds.get(extent.pv_index) else {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!(
                    "logical volume '{}' extent references missing PV index {}",
                    lv.name, extent.pv_index
                ),
            });
        };
        let data_end =
            data_start
                .checked_add(*data_size)
                .ok_or_else(|| LvmError::MetadataParseError {
                    line: 0,
                    message: format!("PV '{}' data area end overflows", pv_name),
                })?;
        let extent_end = extent
            .physical_offset
            .checked_add(extent.length)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: format!("logical volume '{}' physical extent overflows", lv.name),
            })?;
        if extent.physical_offset < *data_start || extent_end > data_end {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!(
                    "logical volume '{}' extent {}..{} falls outside PV '{}' data area {}..{} (pv size {})",
                    lv.name, extent.physical_offset, extent_end, pv_name, data_start, data_end, pv_size
                ),
            });
        }
    }
    if expected != lv.size_bytes {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "logical volume '{}' extent map covers {} bytes but LV size is {}",
                lv.name, expected, lv.size_bytes
            ),
        });
    }
    Ok(())
}

fn lv_info_from_meta(lv: &LvMeta) -> LvInfo {
    let visible = lv.is_visible();
    let directly_mappable = lv.is_directly_mappable();
    LvInfo {
        name: lv.name.clone(),
        uuid: lv.uuid.clone(),
        size_bytes: lv.size_bytes,
        role: lv.role.as_str().to_string(),
        status: lv.status.clone(),
        visible,
        directly_mappable,
        unsupported_reason: if directly_mappable {
            None
        } else {
            let unsupported_segments = lv
                .segments
                .iter()
                .filter_map(unsupported_segment_label)
                .collect::<Vec<_>>();
            if !visible {
                Some("logical volume is hidden or internal".to_string())
            } else if !unsupported_segments.is_empty() {
                Some(format!(
                    "logical volume uses unsupported segment(s): {}",
                    unsupported_segments.join(", ")
                ))
            } else if matches!(lv.role, crate::metadata::LvRole::Snapshot) {
                Some("snapshot logical volume requires origin/COW mapping".to_string())
            } else {
                Some(format!(
                    "logical volume role '{}' is not directly mappable",
                    lv.role.as_str()
                ))
            }
        },
    }
}

fn unsupported_segment_label(segment: &crate::metadata::SegmentMeta) -> Option<String> {
    match &segment.seg_type {
        crate::metadata::SegmentType::Unsupported { type_name } => {
            Some(unsupported_label_with_area_hint(type_name, segment))
        }
        crate::metadata::SegmentType::ThinVolume => Some("thin".to_string()),
        crate::metadata::SegmentType::ThinPool => Some("thin-pool".to_string()),
        crate::metadata::SegmentType::Snapshot => Some("snapshot".to_string()),
        crate::metadata::SegmentType::CacheVolume => Some("cache".to_string()),
        crate::metadata::SegmentType::CachePool => Some("cache-pool".to_string()),
        crate::metadata::SegmentType::Raid0 { .. } => Some("raid0".to_string()),
        crate::metadata::SegmentType::Raid1 { .. } => Some("raid1".to_string()),
        crate::metadata::SegmentType::Raid10 { .. } => Some("raid10".to_string()),
        crate::metadata::SegmentType::Raid5 { .. } => Some("raid5".to_string()),
        crate::metadata::SegmentType::Raid6 { .. } => Some("raid6".to_string()),
        crate::metadata::SegmentType::Linear | crate::metadata::SegmentType::Striped { .. } => None,
    }
}

fn unsupported_label_with_area_hint(
    type_name: &str,
    segment: &crate::metadata::SegmentMeta,
) -> String {
    let component_lvs = segment
        .areas
        .iter()
        .filter_map(|area| match area {
            crate::metadata::SegmentArea::LogicalVolume { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let dependency_lvs = component_graph_lvs(segment);
    if component_lvs.is_empty() && dependency_lvs.is_empty() {
        return type_name.to_string();
    }
    let mut hints = Vec::new();
    if !component_lvs.is_empty() {
        hints.push(format!("areas={}", component_lvs.join(", ")));
    }
    if !dependency_lvs.is_empty() {
        hints.push(format!("dependencies={}", dependency_lvs.join(", ")));
    }
    format!("{} (component LV graph: {})", type_name, hints.join("; "))
}

fn component_graph_lvs(segment: &crate::metadata::SegmentMeta) -> Vec<&str> {
    let mut lvs = segment.dependencies.referenced_lvs();
    lvs.sort_unstable();
    lvs.dedup();
    lvs
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
    extent_size = 1

    physical_volumes {{
        pv0 {{
            id = "{}"
            device = "/dev/sda1"
            pe_start = 5
            pe_count = 4096
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
            pe_start = 5
            pe_count = 16
        }}
        pv1 {{
            id = "{}"
            device = "/dev/sdb1"
            pe_start = 5
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
                stripe_size = 1
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

    fn refresh_label_crc(disk: &mut [u8]) {
        let sec = &mut disk[512..1024];
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
            let mda_size = (text_bytes.len() as u64 + 1024).next_power_of_two();
            mda[32..40].copy_from_slice(&mda_size.to_le_bytes());

            let rl_base: usize = 40;
            mda[rl_base..rl_base + 8].copy_from_slice(&512u64.to_le_bytes());
        }

        disk[text_offset..text_end].copy_from_slice(text_bytes);
        if text_end < disk.len() {
            disk[text_end..].fill(0);
        }

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

        disk[512 + 112..512 + 120]
            .copy_from_slice(&((text_bytes.len() as u64 + 1024).next_power_of_two()).to_le_bytes());
        refresh_label_crc(disk);
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
        assert_eq!(vg.extent_size, 1);
        assert_eq!(vg.seqno, 1);
        assert_eq!(vg.physical_volumes.len(), 1);
        assert_eq!(vg.physical_volumes[0].name, "pv0");

        let vols = pool.list_volumes();
        assert_eq!(vols.len(), 1);
        assert_eq!(vols[0].name, "root");
        assert_eq!(vols[0].size_bytes, 5 * 512); // 5 extents
        assert_eq!(vols[0].role, "public");
        assert!(vols[0].visible);
        assert!(vols[0].directly_mappable);
        assert!(vols[0].unsupported_reason.is_none());
        let direct = pool.list_direct_volumes();
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].0, 0);
        assert_eq!(direct[0].1.name, "root");
        assert_eq!(pool.physical_volume_offsets(), &[("pv0".to_string(), 0)]);
        assert_eq!(
            pool.pv_data_offsets,
            vec![("pv0".to_string(), SYNTHETIC_DATA_AREA_START)]
        );
    }

    #[test]
    fn discover_matches_pv_uuid_case_insensitively() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = format!(
            r#"test_vg {{
id="vg-case-insensitive"
seqno=2
extent_size=1
physical_volumes {{ pv0 {{ id="{}" pe_start=5 pe_count=10 }} }}
logical_volumes {{
root {{ id="lv-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=5 type="linear" stripe_count=1 stripes=["pv0",0] }} }}
}}
}}
"#,
            "ABCDEF1234567890ABCDEF1234567890"
        );
        write_synthetic_metadata(&mut disk, &metadata_text);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();

        assert_eq!(pool.list_direct_volumes().len(), 1);
    }

    #[test]
    fn discover_uses_label_data_area_as_authoritative_offset() {
        let mut disk = build_synthetic_lvm_disk();
        disk[512 + 72..512 + 80].copy_from_slice(&(5u64 * 512).to_le_bytes());
        refresh_label_crc(&mut disk);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();

        assert_eq!(
            pool.physical_volume_data_offsets(),
            &[("pv0".to_string(), 5 * 512)]
        );
    }

    #[test]
    fn discover_fails_when_label_data_area_disagrees_with_metadata_pe_start() {
        let mut disk = build_synthetic_lvm_disk();
        disk[512 + 72..512 + 80].copy_from_slice(&(6u64 * 512).to_le_bytes());
        refresh_label_crc(&mut disk);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let err = match LvmPool::discover(vec![reader], vec![0]) {
            Ok(_) => panic!("mismatched label data area should fail"),
            Err(err) => err,
        };

        assert!(matches!(err, LvmError::MetadataParseError { .. }));
        assert!(err.to_string().contains("data area mismatch"));
    }

    #[test]
    fn discover_fails_when_first_extent_starts_outside_label_data_area() {
        let mut disk = build_synthetic_lvm_disk();
        disk[512 + 80..512 + 88].copy_from_slice(&512u64.to_le_bytes());
        refresh_label_crc(&mut disk);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let err = match LvmPool::discover(vec![reader], vec![0]) {
            Ok(_) => panic!("out-of-bounds first extent should fail"),
            Err(err) => err,
        };

        assert!(matches!(err, LvmError::MetadataParseError { .. }));
        assert!(err.to_string().contains("falls outside PV"));
    }

    #[test]
    fn list_direct_volumes_filters_internal_and_unsupported_lvs() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = format!(
            r#"test_vg {{
id="vg-filtered-lvs"
seqno=3
extent_size=1
physical_volumes {{ pv0 {{ id="{}" pe_start=5 pe_count=4096 }} }}
logical_volumes {{
root {{ id="lv-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="striped" stripe_count=1 stripes=["pv0",0] }} }}
pool_tdata {{ id="lv-tdata" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="striped" stripe_count=1 stripes=["pv0",1] }} }}
thin_root {{ id="lv-thin-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="thin" thin_pool="pool" transaction_id=1 device_id=2 }} }}
}}
}}
"#,
            "abcdef1234567890abcdef1234567890"
        );
        write_synthetic_metadata(&mut disk, &metadata_text);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();
        let all = pool.list_volumes();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].name, "root");
        assert!(all[0].directly_mappable);
        assert_eq!(all[1].name, "pool_tdata");
        assert_eq!(all[1].role, "thin-data");
        assert!(!all[1].visible);
        assert!(!all[1].directly_mappable);
        assert_eq!(all[2].name, "thin_root");
        assert_eq!(all[2].role, "thin");
        assert!(all[2].visible);
        assert!(!all[2].directly_mappable);

        let direct = pool.list_direct_volumes();
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].0, 0);
        assert_eq!(direct[0].1.name, "root");
    }

    #[test]
    fn unsupported_reason_preserves_advanced_segment_dependencies() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = format!(
            r#"test_vg {{
id="vg-advanced-diagnostics"
seqno=7
extent_size=1
physical_volumes {{ pv0 {{ id="{}" pe_start=5 pe_count=4096 }} }}
logical_volumes {{
pool_tmeta {{ id="lv-pool-tmeta" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",0] }} }}
pool_tdata {{ id="lv-pool-tdata" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=4 type="linear" stripe_count=1 stripes=["pv0",1] }} }}
thin_pool {{ id="lv-thin-pool" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=4 type="thin-pool" metadata="pool_tmeta" pool="pool_tdata" transaction_id=1 chunk_size=128 }} }}
thin_root {{ id="lv-thin-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="thin" thin_pool="thin_pool" transaction_id=1 device_id=7 }} }}
origin {{ id="lv-origin" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",10] }} }}
cache_cmeta {{ id="lv-cache-cmeta" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",12] }} }}
cache_cdata {{ id="lv-cache-cdata" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",13] }} }}
cache_pool {{ id="lv-cache-pool" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="cache-pool" metadata="cache_cmeta" data="cache_cdata" chunk_size=64 }} }}
cached_root {{ id="lv-cached-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="cache" cache_pool="cache_pool" origin="origin" }} }}
snap_cow {{ id="lv-snap-cow" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",15] }} }}
origin_snap {{ id="lv-origin-snap" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="snapshot" origin="origin" cow_store="snap_cow" chunk_size=8 }} }}
root_rmeta_0 {{ id="lv-root-rmeta-0" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",17] }} }}
root_rimage_0 {{ id="lv-root-rimage-0" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",18] }} }}
root_rmeta_1 {{ id="lv-root-rmeta-1" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",20] }} }}
root_rimage_1 {{ id="lv-root-rimage-1" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",21] }} }}
mirrored_root {{ id="lv-mirrored-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="raid1" device_count=2 raids=["root_rmeta_0","root_rimage_0","root_rmeta_1","root_rimage_1"] }} }}
}}
}}
"#,
            "abcdef1234567890abcdef1234567890"
        );
        write_synthetic_metadata(&mut disk, &metadata_text);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();
        let volumes = pool.list_volumes();

        let thin_root = volumes
            .iter()
            .find(|volume| volume.name == "thin_root")
            .unwrap();
        let thin_reason = thin_root.unsupported_reason.as_deref().unwrap();
        assert!(thin_reason.contains("thin"));
        assert!(thin_reason.contains("dependencies=thin_pool"));

        let thin_pool = volumes
            .iter()
            .find(|volume| volume.name == "thin_pool")
            .unwrap();
        let thin_pool_reason = thin_pool.unsupported_reason.as_deref().unwrap();
        assert_eq!(thin_pool_reason, "logical volume is hidden or internal");

        let cached_root = volumes
            .iter()
            .find(|volume| volume.name == "cached_root")
            .unwrap();
        let cache_reason = cached_root.unsupported_reason.as_deref().unwrap();
        assert!(cache_reason.contains("cache"));
        assert!(cache_reason.contains("areas=origin, cache_pool"));
        assert!(cache_reason.contains("dependencies=cache_pool, origin"));

        let snapshot = volumes
            .iter()
            .find(|volume| volume.name == "origin_snap")
            .unwrap();
        let snapshot_reason = snapshot.unsupported_reason.as_deref().unwrap();
        assert!(snapshot_reason.contains("snapshot"));
        assert!(snapshot_reason.contains("areas=origin, snap_cow"));
        assert!(snapshot_reason.contains("dependencies=origin, snap_cow"));

        let raid = volumes
            .iter()
            .find(|volume| volume.name == "mirrored_root")
            .unwrap();
        let raid_reason = raid.unsupported_reason.as_deref().unwrap();
        assert!(raid_reason.contains("raid1"));
        assert!(
            raid_reason.contains("areas=root_rmeta_0, root_rimage_0, root_rmeta_1, root_rimage_1")
        );
        assert!(raid_reason
            .contains("dependencies=root_rimage_0, root_rimage_1, root_rmeta_0, root_rmeta_1"));

        let direct_names = pool
            .list_direct_volumes()
            .into_iter()
            .map(|(_, volume)| volume.name)
            .collect::<Vec<_>>();
        assert_eq!(direct_names, vec!["origin"]);
    }

    #[test]
    fn list_readable_volumes_includes_supported_thin_lvs_without_changing_direct_list() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = format!(
            r#"test_vg {{
id="vg-readable-thin"
seqno=8
extent_size=1
physical_volumes {{ pv0 {{ id="{}" pe_start=5 pe_count=4096 }} }}
logical_volumes {{
pool_tmeta {{ id="lv-pool-tmeta" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",0] }} }}
pool_tdata {{ id="lv-pool-tdata" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=4 type="linear" stripe_count=1 stripes=["pv0",1] }} }}
thin_pool {{ id="lv-thin-pool" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=4 type="thin-pool" metadata="pool_tmeta" pool="pool_tdata" transaction_id=1 chunk_size=128 }} }}
thin_root {{ id="lv-thin-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="thin" thin_pool="thin_pool" transaction_id=1 device_id=7 }} }}
origin {{ id="lv-origin" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",10] }} }}
}}
}}
"#,
            "abcdef1234567890abcdef1234567890"
        );
        write_synthetic_metadata(&mut disk, &metadata_text);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();

        let direct_names = pool
            .list_direct_volumes()
            .into_iter()
            .map(|(_, volume)| volume.name)
            .collect::<Vec<_>>();
        assert_eq!(direct_names, vec!["origin"]);

        let readable_names = pool
            .list_readable_volumes()
            .into_iter()
            .map(|(_, volume)| volume.name)
            .collect::<Vec<_>>();
        assert_eq!(readable_names, vec!["thin_root", "origin"]);
    }

    #[test]
    fn discover_resolves_component_lv_area_backed_by_physical_volume() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = format!(
            r#"test_vg {{
id="vg-component-area"
seqno=4
extent_size=1
physical_volumes {{ pv0 {{ id="{}" pe_start=5 pe_count=4096 }} }}
logical_volumes {{
component_lv {{ id="lv-component" status=["READ","WRITE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",0] }} }}
direct_root {{ id="lv-direct" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",1] }} }}
component_backed {{ id="lv-component-backed" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["component_lv",0] }} }}
}}
}}
"#,
            "abcdef1234567890abcdef1234567890"
        );
        write_synthetic_metadata(&mut disk, &metadata_text);
        let marker = b"COMPONENT-BACKED";
        disk[SYNTHETIC_DATA_AREA_START as usize..SYNTHETIC_DATA_AREA_START as usize + marker.len()]
            .copy_from_slice(marker);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let pool = LvmPool::discover(vec![reader], vec![0]).unwrap();
        let all = pool.list_volumes();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].name, "component_lv");
        assert!(!all[0].directly_mappable);
        assert_eq!(all[1].name, "direct_root");
        assert!(all[1].directly_mappable);
        assert_eq!(all[2].name, "component_backed");
        assert!(all[2].visible);
        assert!(all[2].directly_mappable);
        assert!(all[2].unsupported_reason.is_none());

        let direct = pool.list_direct_volumes();
        assert_eq!(direct.len(), 2);
        assert_eq!(direct[0].0, 1);
        assert_eq!(direct[0].1.name, "direct_root");
        assert_eq!(direct[1].0, 2);
        assert_eq!(direct[1].1.name, "component_backed");

        let mut lv = pool.open_volume(2).unwrap();
        let mut buf = vec![0u8; marker.len()];
        lv.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, marker);
    }

    #[test]
    fn discover_fails_closed_on_cyclic_component_lv_area() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = format!(
            r#"test_vg {{
id="vg-cycle"
seqno=5
extent_size=1
physical_volumes {{ pv0 {{ id="{}" pe_start=5 pe_count=4096 }} }}
logical_volumes {{
cycle_a {{ id="lv-cycle-a" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["cycle_b",0] }} }}
cycle_b {{ id="lv-cycle-b" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["cycle_a",0] }} }}
}}
}}
"#,
            "abcdef1234567890abcdef1234567890"
        );
        write_synthetic_metadata(&mut disk, &metadata_text);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let err = match LvmPool::discover(vec![reader], vec![0]) {
            Ok(_) => panic!("cyclic component LV graph should fail closed"),
            Err(err) => err,
        };

        assert!(matches!(err, LvmError::MetadataParseError { .. }));
        assert!(err.to_string().contains("cyclic LVM"));
    }

    #[test]
    fn discover_fails_closed_when_component_lv_depends_on_thin_volume() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = format!(
            r#"test_vg {{
id="vg-thin-dependency"
seqno=6
extent_size=1
physical_volumes {{ pv0 {{ id="{}" pe_start=5 pe_count=4096 }} }}
logical_volumes {{
thin_component {{ id="lv-thin-component" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="thin" thin_pool="pool" transaction_id=1 device_id=7 }} }}
component_backed {{ id="lv-component-backed" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["thin_component",0] }} }}
}}
}}
"#,
            "abcdef1234567890abcdef1234567890"
        );
        write_synthetic_metadata(&mut disk, &metadata_text);

        let reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(disk));
        let err = match LvmPool::discover(vec![reader], vec![0]) {
            Ok(_) => panic!("component graph through thin volume should fail closed"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("thin"), "unexpected error: {err}");
    }

    #[test]
    fn discover_matches_label_uuid_to_dashed_metadata_uuid() {
        let mut disk = build_synthetic_lvm_disk();
        let metadata_text = r#"test_vg {
    id = "vg-dashed-pv-uuid"
    seqno = 2
    extent_size = 1

    physical_volumes {
        pv0 {
            id = "abcdef12-3456-7890-abcd-ef1234567890"
            device = "/dev/sda1"
            pe_start = 5
            pe_count = 4096
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
        let data_area_start = SYNTHETIC_DATA_AREA_START as usize;
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

    #[test]
    fn discover_uses_complete_lower_seqno_copy_when_higher_copy_is_incomplete() {
        let complete_metadata = format!(
            r#"test_vg {{
id="vg-complete-lower"
seqno=10
extent_size=1
physical_volumes {{
pv0 {{ id="{}" pe_start=5 pe_count=16 }}
pv1 {{ id="{}" pe_start=5 pe_count=16 }}
}}
logical_volumes {{
root {{ id="lv-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 {{ start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",0] }} }}
}}
}}
"#,
            PV0_UUID, PV1_UUID
        );
        let incomplete_metadata = complete_metadata.replace("seqno=10", "seqno=99").replace(
            "pv0 { id=\"00000000000000000000000000000000\" pe_start=5 pe_count=16 }",
            "pv0 { id=\"00000000000000000000000000000000\" pe_count=16 }",
        );
        let mut pv0 = vec![0u8; SYNTHETIC_PV_SIZE as usize];
        let mut pv1 = vec![0u8; SYNTHETIC_PV_SIZE as usize];
        write_synthetic_pv_label(&mut pv0, PV0_UUID);
        write_synthetic_pv_label(&mut pv1, PV1_UUID);
        write_synthetic_metadata(&mut pv0, &incomplete_metadata);
        write_synthetic_metadata(&mut pv1, &complete_metadata);

        let pv0_reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(pv0));
        let pv1_reader: Box<dyn EvidenceReader> = Box::new(FakeDiskReader::new(pv1));
        let pool = LvmPool::discover(vec![pv0_reader, pv1_reader], vec![0, 0]).unwrap();

        assert_eq!(pool.volume_group().seqno, 10);
        assert_eq!(pool.list_direct_volumes().len(), 1);
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
