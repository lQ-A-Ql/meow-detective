/// Logical-to-physical extent mapping engine.
///
/// For a linear logical volume (stripe_count=1), mapping is straightforward:
///   physical_offset = pv_data_area_start + (stripe_pe_start + le_index) * extent_size_bytes
///
/// For striped logical volumes (stripe_count > 1), extents are interleaved
/// across multiple PVs according to the stripe width.
use crate::error::{LvmError, Result};
use crate::metadata::{LvMeta, SegmentMeta, SegmentType, VolumeGroup};

/// A resolved extent mapping: a contiguous range of bytes on a single PV.
#[derive(Debug, Clone)]
pub struct LvExtent {
    /// Byte offset within the logical volume.
    pub logical_start: u64,
    /// Absolute byte offset on the physical device.
    pub physical_offset: u64,
    /// Length of this extent in bytes.
    pub length: u64,
    /// Index into the PV reader array (= 0 for single-PV setups).
    pub pv_index: usize,
}

/// Build the complete extent map for a logical volume.
///
/// `pv_data_offsets`: for each PV in `vg.physical_volumes`, the absolute
/// byte offset where its data area starts (typically `2048 * 512`).
pub fn build_extent_map(
    vg: &VolumeGroup,
    lv: &LvMeta,
    pv_data_offsets: &[(String, u64)],
) -> Result<Vec<LvExtent>> {
    let extent_size_bytes = vg.extent_size * 512;
    let mut map = Vec::new();
    let mut le_cursor: u64 = 0; // current logical extent index

    for segment in &lv.segments {
        match &segment.seg_type {
            SegmentType::Linear => {
                map.extend(build_linear(
                    segment,
                    le_cursor,
                    extent_size_bytes,
                    pv_data_offsets,
                )?);
                le_cursor += segment.extent_count;
            }
            SegmentType::Striped { stripe_count } => {
                let stripe_extents = build_striped(
                    segment,
                    le_cursor,
                    extent_size_bytes,
                    *stripe_count,
                    pv_data_offsets,
                )?;
                le_cursor += segment.extent_count;
                map.extend(stripe_extents);
            }
            SegmentType::Raid0 { stripe_count } => {
                // RAID 0 is identical to striped — data is interleaved across
                // PVs with no parity. Use the same mapping as Striped.
                let stripe_extents = build_striped(
                    segment,
                    le_cursor,
                    extent_size_bytes,
                    *stripe_count,
                    pv_data_offsets,
                )?;
                le_cursor += segment.extent_count;
                map.extend(stripe_extents);
            }
            SegmentType::Raid1 { .. } | SegmentType::Raid10 { .. } => {
                // RAID 1 (mirroring): read from the first mirror copy.
                // RAID 10 (striped mirrors): read from first mirror in each stripe.
                // For forensics, any complete copy suffices — we use the first PV.
                if segment.stripes.is_empty() {
                    return Err(LvmError::MetadataParseError {
                        line: 0,
                        message: format!("RAID segment in LV '{}' has no stripes", lv.name),
                    });
                }
                // Use only the first copy: map as linear on the first PV in stripes
                let (pv_name, stripe_pe_start) = &segment.stripes[0];
                let pv_index = find_pv_index(pv_data_offsets, pv_name)?;
                let pv_data_start = pv_data_offsets[pv_index].1;
                let logical_start = le_cursor * extent_size_bytes;
                let physical_offset = pv_data_start + stripe_pe_start * extent_size_bytes;
                let length = segment.extent_count * extent_size_bytes;
                le_cursor += segment.extent_count;
                map.push(LvExtent {
                    logical_start,
                    physical_offset,
                    length,
                    pv_index,
                });
            }
            SegmentType::Raid5 { .. } | SegmentType::Raid6 { .. } => {
                return Err(LvmError::UnsupportedSegment {
                    lv_name: lv.name.clone(),
                    seg_type: "raid5/raid6 (parity RAID requires reconstruction logic)".into(),
                });
            }
            SegmentType::ThinPool
            | SegmentType::Snapshot
            | SegmentType::CachePool
            | SegmentType::Unsupported { .. } => {
                let name = match &segment.seg_type {
                    SegmentType::ThinPool => "thin-pool",
                    SegmentType::Snapshot => "snapshot",
                    SegmentType::CachePool => "cache-pool",
                    SegmentType::Unsupported { type_name } => type_name.as_str(),
                    _ => unreachable!(),
                };
                return Err(LvmError::UnsupportedSegment {
                    lv_name: lv.name.clone(),
                    seg_type: name.to_string(),
                });
            }
        }
    }

    Ok(map)
}

/// Build extent mappings for a linear segment.
///
/// A linear segment maps a contiguous range of logical extents to a single
/// contiguous range of physical extents on one PV.
fn build_linear(
    segment: &SegmentMeta,
    base_le: u64,
    extent_size_bytes: u64,
    pv_data_offsets: &[(String, u64)],
) -> Result<Vec<LvExtent>> {
    if segment.stripes.is_empty() {
        return Ok(Vec::new());
    }

    let (pv_name, stripe_pe_start) = &segment.stripes[0];
    let pv_index = find_pv_index(pv_data_offsets, pv_name)?;
    let pv_data_start = pv_data_offsets[pv_index].1;

    let logical_start = base_le * extent_size_bytes;
    let physical_offset = pv_data_start + stripe_pe_start * extent_size_bytes;
    let length = segment.extent_count * extent_size_bytes;

    Ok(vec![LvExtent {
        logical_start,
        physical_offset,
        length,
        pv_index,
    }])
}

/// Build extent mappings for a striped segment.
///
/// Striped segments interleave extents across multiple PVs:
///   LE 0 → PV0, LE 1 → PV1, ..., LE N-1 → PV(N-1), LE N → PV0, ...
fn build_striped(
    segment: &SegmentMeta,
    base_le: u64,
    extent_size_bytes: u64,
    stripe_count: u64,
    pv_data_offsets: &[(String, u64)],
) -> Result<Vec<LvExtent>> {
    // Build a lookup table: stripe_index → (pv_index, pv_data_start)
    let stripe_pvs: Vec<(usize, u64)> = (0..stripe_count)
        .map(|si| {
            // The stripes list may have fewer entries than stripe_count
            // in some metadata formats; default to using the first PV for
            // unspecified stripes.
            let pv_name = if (si as usize) < segment.stripes.len() {
                &segment.stripes[si as usize].0
            } else {
                &segment.stripes[0].0
            };
            let pv_idx = find_pv_index(pv_data_offsets, pv_name)?;
            Ok((pv_idx, pv_data_offsets[pv_idx].1))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut map = Vec::new();
    for le_offset in 0..segment.extent_count {
        let le = base_le + le_offset;
        let stripe_idx = (le_offset % stripe_count) as usize;
        let (pv_index, pv_data_start) = stripe_pvs[stripe_idx];

        // Determine which stripe entry corresponds to this PV
        let stripe_pe_start = if stripe_idx < segment.stripes.len() {
            segment.stripes[stripe_idx].1
        } else {
            segment.stripes[0].1
        };

        let logical_start = le * extent_size_bytes;
        let physical_offset =
            pv_data_start + (stripe_pe_start + le_offset / stripe_count) * extent_size_bytes;

        map.push(LvExtent {
            logical_start,
            physical_offset,
            length: extent_size_bytes,
            pv_index,
        });
    }

    Ok(map)
}

fn find_pv_index(pv_data_offsets: &[(String, u64)], pv_name: &str) -> Result<usize> {
    pv_data_offsets
        .iter()
        .position(|(name, _)| name == pv_name)
        .ok_or_else(|| LvmError::UnknownPhysicalVolume {
            name: pv_name.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{LvMeta, PvMeta, SegmentMeta, SegmentType, VolumeGroup};

    fn make_test_vg() -> VolumeGroup {
        VolumeGroup {
            name: "test_vg".into(),
            id: "vg-id".into(),
            extent_size: 8192, // 4 MB extents
            seqno: 1,
            physical_volumes: vec![PvMeta {
                name: "pv0".into(),
                uuid: "pv-uuid".into(),
                pe_start: 2048,
                pe_count: 2560,
            }],
            logical_volumes: Vec::new(),
        }
    }

    #[test]
    fn linear_single_pv_mapping() {
        let vg = make_test_vg();
        let lv = LvMeta {
            name: "root".into(),
            uuid: "lv-uuid".into(),
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 10,
                seg_type: SegmentType::Linear,
                stripes: vec![("pv0".into(), 0)],
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512)];

        let map = build_extent_map(&vg, &lv, &pv_offsets).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].logical_start, 0);
        assert_eq!(map[0].physical_offset, 2048 * 512); // pv_data_start
        assert_eq!(map[0].length, 10 * 8192 * 512);
        assert_eq!(map[0].pv_index, 0);
    }

    #[test]
    fn linear_with_offset() {
        let vg = make_test_vg();
        let lv = LvMeta {
            name: "home".into(),
            uuid: "lv-uuid2".into(),
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 5,
                seg_type: SegmentType::Linear,
                stripes: vec![("pv0".into(), 1280)], // starts at PE 1280
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512)];

        let map = build_extent_map(&vg, &lv, &pv_offsets).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].physical_offset, 2048 * 512 + 1280 * 8192 * 512);
    }

    #[test]
    fn unsupported_segment_type() {
        let vg = make_test_vg();
        let lv = LvMeta {
            name: "cache".into(),
            uuid: "lv-uuid3".into(),
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 10,
                seg_type: SegmentType::Unsupported {
                    type_name: "thin-pool".into(),
                },
                stripes: vec![],
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512)];

        let err = build_extent_map(&vg, &lv, &pv_offsets).unwrap_err();
        assert!(err.to_string().contains("thin-pool"));
    }

    #[test]
    fn striped_two_pv_mapping() {
        let vg = VolumeGroup {
            name: "test_vg".into(),
            id: "vg-id".into(),
            extent_size: 8192,
            seqno: 1,
            physical_volumes: vec![
                PvMeta {
                    name: "pv0".into(),
                    uuid: "pv0-uuid".into(),
                    pe_start: 2048,
                    pe_count: 2560,
                },
                PvMeta {
                    name: "pv1".into(),
                    uuid: "pv1-uuid".into(),
                    pe_start: 2048,
                    pe_count: 2560,
                },
            ],
            logical_volumes: Vec::new(),
        };
        let lv = LvMeta {
            name: "striped_lv".into(),
            uuid: "lv-uuid".into(),
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 4, // 4 LEs, 2 per PV
                seg_type: SegmentType::Striped { stripe_count: 2 },
                stripes: vec![("pv0".into(), 0), ("pv1".into(), 0)],
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![
            ("pv0".into(), 2048 * 512),
            ("pv1".into(), 2048 * 512),
        ];

        let map = build_extent_map(&vg, &lv, &pv_offsets).unwrap();
        assert_eq!(map.len(), 4, "4 logical extents → 4 extent entries");
        // LE 0 → PV 0
        assert_eq!(map[0].logical_start, 0);
        assert_eq!(map[0].pv_index, 0);
        assert_eq!(map[0].physical_offset, 2048 * 512); // pv0 data start
        // LE 1 → PV 1
        assert_eq!(map[1].logical_start, 1 * 8192 * 512);
        assert_eq!(map[1].pv_index, 1);
        assert_eq!(map[1].physical_offset, 2048 * 512); // pv1 data start
        // LE 2 → PV 0
        assert_eq!(map[2].logical_start, 2 * 8192 * 512);
        assert_eq!(map[2].pv_index, 0);
        assert_eq!(map[2].physical_offset, 2048 * 512 + 1 * 8192 * 512);
        // LE 3 → PV 1
        assert_eq!(map[3].logical_start, 3 * 8192 * 512);
        assert_eq!(map[3].pv_index, 1);
        assert_eq!(map[3].physical_offset, 2048 * 512 + 1 * 8192 * 512);
    }

    #[test]
    fn multi_segment_linear() {
        // Two contiguous segments: first 5 LEs on PV0, next 3 LEs also on PV0
        let vg = make_test_vg();
        let lv = LvMeta {
            name: "multi_seg".into(),
            uuid: "lv-uuid".into(),
            segments: vec![
                SegmentMeta {
                    start_extent: 0,
                    extent_count: 5,
                    seg_type: SegmentType::Linear,
                    stripes: vec![("pv0".into(), 0)],
                },
                SegmentMeta {
                    start_extent: 0,
                    extent_count: 3,
                    seg_type: SegmentType::Linear,
                    stripes: vec![("pv0".into(), 128)],
                },
            ],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512)];

        let map = build_extent_map(&vg, &lv, &pv_offsets).unwrap();
        assert_eq!(map.len(), 2);
        // Segment 1: 5 extents
        assert_eq!(map[0].logical_start, 0);
        assert_eq!(map[0].length, 5 * 8192 * 512);
        // Segment 2: 3 extents (starts after segment 1)
        assert_eq!(map[1].logical_start, 5 * 8192 * 512);
        assert_eq!(map[1].physical_offset, 2048 * 512 + 128 * 8192 * 512);
        assert_eq!(map[1].length, 3 * 8192 * 512);
    }

    #[test]
    fn raid0_same_as_striped() {
        let vg = make_test_vg();
        let lv = LvMeta {
            name: "raid0_lv".into(),
            uuid: "lv-uuid".into(),
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 2,
                seg_type: SegmentType::Raid0 { stripe_count: 2 },
                stripes: vec![("pv0".into(), 0), ("pv0".into(), 100)],
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512)];
        let map = build_extent_map(&vg, &lv, &pv_offsets).unwrap();
        assert_eq!(map.len(), 2, "RAID0 with 2 stripes → 2 extent entries");
    }

    #[test]
    fn raid1_reads_first_mirror() {
        let vg = VolumeGroup {
            name: "mirror_vg".into(),
            id: "vg-id".into(),
            extent_size: 4096,
            seqno: 1,
            physical_volumes: vec![
                PvMeta { name: "pv0".into(), uuid: "pv0-uuid".into(), pe_start: 2048, pe_count: 1000 },
                PvMeta { name: "pv1".into(), uuid: "pv1-uuid".into(), pe_start: 2048, pe_count: 1000 },
            ],
            logical_volumes: Vec::new(),
        };
        let lv = LvMeta {
            name: "mirrored_lv".into(),
            uuid: "lv-uuid".into(),
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 10,
                seg_type: SegmentType::Raid1 { mirror_count: 2 },
                stripes: vec![("pv0".into(), 0), ("pv1".into(), 0)],
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512), ("pv1".into(), 4096 * 512)];

        let map = build_extent_map(&vg, &lv, &pv_offsets).unwrap();
        // RAID1: should only map to the first mirror (pv0)
        assert_eq!(map.len(), 1, "RAID1 maps to single mirror copy");
        assert_eq!(map[0].pv_index, 0);
        assert_eq!(map[0].physical_offset, 2048 * 512); // pv0 data start
        assert_eq!(map[0].length, 10 * 4096 * 512);
    }
}
