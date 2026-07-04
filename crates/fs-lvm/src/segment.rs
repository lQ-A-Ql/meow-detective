/// Logical-to-physical extent mapping engine.
///
/// For a linear logical volume (stripe_count=1), mapping is straightforward:
///   physical_offset = pv_data_area_start + (stripe_pe_start + le_index) * extent_size_bytes
///
/// For striped logical volumes (stripe_count > 1), extents are interleaved
/// across multiple PVs according to the stripe width.
use crate::error::{LvmError, Result};
use crate::metadata::{LvMeta, SegmentArea, SegmentMeta, SegmentType, VolumeGroup};

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

struct MapContext<'a> {
    vg: &'a VolumeGroup,
    pv_data_offsets: &'a [(String, u64)],
    extent_size_bytes: u64,
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
    let mut stack = vec![lv.name.clone()];
    let extent_size_bytes =
        vg.extent_size
            .checked_mul(512)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: format!("extent size overflows bytes for VG '{}'", vg.name),
            })?;
    let ctx = MapContext {
        vg,
        pv_data_offsets,
        extent_size_bytes,
    };
    build_extent_map_inner(&ctx, lv, &mut stack)
}

fn build_extent_map_inner(
    ctx: &MapContext<'_>,
    lv: &LvMeta,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    let mut map = Vec::new();

    for segment in &lv.segments {
        let base_le = segment.start_extent;
        match &segment.seg_type {
            SegmentType::Linear => {
                map.extend(build_linear(ctx, segment, base_le, stack)?);
            }
            SegmentType::Striped {
                stripe_count,
                stripe_size,
            } => {
                let stripe_extents =
                    build_striped(ctx, segment, base_le, *stripe_count, *stripe_size, stack)?;
                map.extend(stripe_extents);
            }
            SegmentType::Raid0 { .. } => {
                return Err(LvmError::UnsupportedSegment {
                    lv_name: lv.name.clone(),
                    seg_type: "raid0 (requires LVM2 raid0_lvs/raids component LV mapping)"
                        .to_string(),
                });
            }
            SegmentType::Raid1 { .. } | SegmentType::Raid10 { .. } => {
                return Err(LvmError::UnsupportedSegment {
                    lv_name: lv.name.clone(),
                    seg_type: "raid1/raid10 (requires LVM component LV graph mapping)".into(),
                });
            }
            SegmentType::Raid5 { .. } | SegmentType::Raid6 { .. } => {
                return Err(LvmError::UnsupportedSegment {
                    lv_name: lv.name.clone(),
                    seg_type: "raid5/raid6 (parity RAID requires reconstruction logic)".into(),
                });
            }
            SegmentType::ThinVolume
            | SegmentType::ThinPool
            | SegmentType::Snapshot
            | SegmentType::CacheVolume
            | SegmentType::CachePool
            | SegmentType::Unsupported { .. } => {
                let name = match &segment.seg_type {
                    SegmentType::ThinVolume => "thin",
                    SegmentType::ThinPool => "thin-pool",
                    SegmentType::Snapshot => "snapshot",
                    SegmentType::CacheVolume => "cache",
                    SegmentType::CachePool => "cache-pool",
                    SegmentType::Unsupported { type_name } => type_name.as_str(),
                    _ => unreachable!(),
                };
                return Err(LvmError::UnsupportedSegment {
                    lv_name: lv.name.clone(),
                    seg_type: format!("{name} (dependency chain: {})", stack.join(" -> ")),
                });
            }
        }
    }

    map.sort_by_key(|extent| extent.logical_start);
    Ok(map)
}

/// Build extent mappings for a linear segment.
///
/// A linear segment maps a contiguous range of logical extents to a single
/// contiguous range of physical extents on one PV.
fn build_linear(
    ctx: &MapContext<'_>,
    segment: &SegmentMeta,
    base_le: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    if segment.areas.is_empty() {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: "linear segment has no data areas".to_string(),
        });
    }
    if segment.areas.len() != 1 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "linear segment expected 1 data area but found {}",
                segment.areas.len()
            ),
        });
    }

    let logical_start = checked_mul(base_le, ctx.extent_size_bytes, "linear logical start")?;
    let length = checked_mul(
        segment.extent_count,
        ctx.extent_size_bytes,
        "linear segment length",
    )?;

    map_area_range(ctx, &segment.areas[0], logical_start, 0, length, stack)
}

/// Build extent mappings for a striped segment.
///
/// Striped segments interleave extents across multiple PVs:
///   LE 0 → PV0, LE 1 → PV1, ..., LE N-1 → PV(N-1), LE N → PV0, ...
fn build_striped(
    ctx: &MapContext<'_>,
    segment: &SegmentMeta,
    base_le: u64,
    stripe_count: u64,
    stripe_size_sectors: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    validate_stripe_count(segment, stripe_count, "striped")?;
    let stripe_size_bytes =
        checked_mul(stripe_size_sectors, 512, "striped stripe_size byte length")?;
    if stripe_size_bytes == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: "striped segment has stripe_size 0".to_string(),
        });
    }

    let mut map = Vec::new();
    let logical_start = checked_mul(base_le, ctx.extent_size_bytes, "striped logical start")?;
    let segment_len = checked_mul(
        segment.extent_count,
        ctx.extent_size_bytes,
        "striped segment length",
    )?;
    let mut segment_offset = 0u64;

    while segment_offset < segment_len {
        let chunk_number = segment_offset / stripe_size_bytes;
        let stripe_idx = (chunk_number % stripe_count) as usize;
        let stripe_set = chunk_number / stripe_count;
        let in_chunk_offset = segment_offset % stripe_size_bytes;
        let remaining_in_chunk = stripe_size_bytes - in_chunk_offset;
        let remaining_in_segment = segment_len - segment_offset;
        let length = remaining_in_chunk.min(remaining_in_segment);

        let stripe_set_offset =
            checked_mul(stripe_set, stripe_size_bytes, "striped set byte offset")?;
        let area_offset = checked_add(
            stripe_set_offset,
            in_chunk_offset,
            "striped in-chunk offset",
        )?;
        let outer_logical_start =
            checked_add(logical_start, segment_offset, "striped logical chunk start")?;
        map.extend(map_area_range(
            ctx,
            &segment.areas[stripe_idx],
            outer_logical_start,
            area_offset,
            length,
            stack,
        )?);

        segment_offset = checked_add(segment_offset, length, "striped segment cursor")?;
    }

    Ok(map)
}

fn map_area_range(
    ctx: &MapContext<'_>,
    area: &SegmentArea,
    outer_logical_start: u64,
    area_relative_offset: u64,
    length: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    match area {
        SegmentArea::PhysicalVolume { name, start_extent } => {
            let pv_index = find_pv_index(ctx.pv_data_offsets, name)?;
            let pv_data_start = ctx.pv_data_offsets[pv_index].1;
            let area_start = checked_mul(*start_extent, ctx.extent_size_bytes, "PV area start")?;
            let physical_offset = checked_add(
                checked_add(pv_data_start, area_start, "PV area physical start")?,
                area_relative_offset,
                "PV area relative offset",
            )?;
            Ok(vec![LvExtent {
                logical_start: outer_logical_start,
                physical_offset,
                length,
                pv_index,
            }])
        }
        SegmentArea::LogicalVolume { name, start_extent } => map_logical_volume_area(
            ctx,
            name,
            *start_extent,
            outer_logical_start,
            area_relative_offset,
            length,
            stack,
        ),
        SegmentArea::Unassigned { .. } => Err(LvmError::UnsupportedSegment {
            lv_name: stack
                .last()
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_string()),
            seg_type: "unassigned segment area".to_string(),
        }),
    }
}

fn map_logical_volume_area(
    ctx: &MapContext<'_>,
    name: &str,
    start_extent: u64,
    outer_logical_start: u64,
    area_relative_offset: u64,
    length: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    if stack.iter().any(|entry| entry == name) {
        let mut cycle = stack.join(" -> ");
        cycle.push_str(" -> ");
        cycle.push_str(name);
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("cyclic LVM logical-volume area reference: {cycle}"),
        });
    }

    let target = ctx
        .vg
        .logical_volumes
        .iter()
        .find(|lv| lv.name == name)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("unknown logical volume '{name}' referenced in segment mapping"),
        })?;

    stack.push(name.to_string());
    let target_map = build_extent_map_inner(ctx, target, stack);
    stack.pop();
    let target_map = target_map?;

    let source_start = checked_add(
        checked_mul(
            start_extent,
            ctx.extent_size_bytes,
            "logical-volume area start",
        )?,
        area_relative_offset,
        "logical-volume area relative offset",
    )?;
    slice_extent_map(&target_map, source_start, length, outer_logical_start, name)
}

fn slice_extent_map(
    extents: &[LvExtent],
    source_start: u64,
    length: u64,
    outer_logical_start: u64,
    source_lv_name: &str,
) -> Result<Vec<LvExtent>> {
    let source_end = checked_add(source_start, length, "logical-volume area end")?;
    let mut cursor = source_start;
    let mut sliced = Vec::new();

    for extent in extents {
        let extent_end = checked_add(extent.logical_start, extent.length, "source extent end")?;
        if extent_end <= cursor || extent.logical_start >= source_end {
            continue;
        }
        if extent.logical_start > cursor {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!(
                    "logical volume '{source_lv_name}' area has uncovered logical range at byte {cursor}"
                ),
            });
        }

        let overlap_start = cursor.max(extent.logical_start);
        let overlap_end = source_end.min(extent_end);
        if overlap_end <= overlap_start {
            continue;
        }
        let offset_in_extent = overlap_start - extent.logical_start;
        let offset_in_area = overlap_start - source_start;
        sliced.push(LvExtent {
            logical_start: checked_add(
                outer_logical_start,
                offset_in_area,
                "sliced logical extent start",
            )?,
            physical_offset: checked_add(
                extent.physical_offset,
                offset_in_extent,
                "sliced physical extent start",
            )?,
            length: overlap_end - overlap_start,
            pv_index: extent.pv_index,
        });
        cursor = overlap_end;
        if cursor == source_end {
            break;
        }
    }

    if cursor != source_end {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "logical volume '{source_lv_name}' area ended before requested byte {source_end}"
            ),
        });
    }

    Ok(sliced)
}

fn find_pv_index(pv_data_offsets: &[(String, u64)], pv_name: &str) -> Result<usize> {
    pv_data_offsets
        .iter()
        .position(|(name, _)| name == pv_name)
        .ok_or_else(|| LvmError::UnknownPhysicalVolume {
            name: pv_name.to_string(),
        })
}

fn validate_stripe_count(
    segment: &SegmentMeta,
    stripe_count: u64,
    segment_kind: &str,
) -> Result<()> {
    if stripe_count == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("{} segment has stripe_count 0", segment_kind),
        });
    }
    if segment_kind == "raid0" && stripe_count < 2 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: "raid0 segment requires at least 2 stripes".to_string(),
        });
    }
    let area_count = if segment.areas.is_empty() {
        segment.stripes.len()
    } else {
        segment.areas.len()
    };
    if area_count == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("{} segment has no data areas", segment_kind),
        });
    }
    if area_count != stripe_count as usize {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "{} segment stripe_count {} does not match {} data area entries",
                segment_kind, stripe_count, area_count
            ),
        });
    }
    Ok(())
}

fn checked_add(lhs: u64, rhs: u64, context: &str) -> Result<u64> {
    lhs.checked_add(rhs)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("{} overflows u64", context),
        })
}

fn checked_mul(lhs: u64, rhs: u64, context: &str) -> Result<u64> {
    lhs.checked_mul(rhs)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("{} overflows u64", context),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{
        LvMeta, PvMeta, SegmentArea, SegmentDependencies, SegmentMeta, SegmentType, VolumeGroup,
    };

    fn pv_area(name: &str, start_extent: u64) -> SegmentArea {
        SegmentArea::PhysicalVolume {
            name: name.to_string(),
            start_extent,
        }
    }

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
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 10,
                seg_type: SegmentType::Linear,
                stripes: vec![("pv0".into(), 0)],
                areas: vec![pv_area("pv0", 0)],
                dependencies: SegmentDependencies::default(),
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
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 5,
                seg_type: SegmentType::Linear,
                stripes: vec![("pv0".into(), 1280)], // starts at PE 1280
                areas: vec![pv_area("pv0", 1280)],
                dependencies: SegmentDependencies::default(),
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
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 10,
                seg_type: SegmentType::Unsupported {
                    type_name: "thin-pool".into(),
                },
                stripes: vec![],
                areas: vec![],
                dependencies: SegmentDependencies::default(),
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
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 4, // 4 LEs, 2 per PV
                seg_type: SegmentType::Striped {
                    stripe_count: 2,
                    stripe_size: 8192,
                },
                stripes: vec![("pv0".into(), 0), ("pv1".into(), 0)],
                areas: vec![pv_area("pv0", 0), pv_area("pv1", 0)],
                dependencies: SegmentDependencies::default(),
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512), ("pv1".into(), 2048 * 512)];

        let map = build_extent_map(&vg, &lv, &pv_offsets).unwrap();
        assert_eq!(map.len(), 4, "4 logical extents → 4 extent entries");
        // LE 0 → PV 0
        assert_eq!(map[0].logical_start, 0);
        assert_eq!(map[0].pv_index, 0);
        assert_eq!(map[0].physical_offset, 2048 * 512); // pv0 data start
                                                        // LE 1 → PV 1
        assert_eq!(map[1].logical_start, 8192 * 512);
        assert_eq!(map[1].pv_index, 1);
        assert_eq!(map[1].physical_offset, 2048 * 512); // pv1 data start
                                                        // LE 2 → PV 0
        assert_eq!(map[2].logical_start, 2 * 8192 * 512);
        assert_eq!(map[2].pv_index, 0);
        assert_eq!(map[2].physical_offset, 2048 * 512 + 8192 * 512);
        // LE 3 → PV 1
        assert_eq!(map[3].logical_start, 3 * 8192 * 512);
        assert_eq!(map[3].pv_index, 1);
        assert_eq!(map[3].physical_offset, 2048 * 512 + 8192 * 512);
    }

    #[test]
    fn striped_mapping_uses_stripe_size_chunks() {
        let vg = VolumeGroup {
            name: "chunk_vg".into(),
            id: "vg-id".into(),
            extent_size: 8,
            seqno: 1,
            physical_volumes: vec![
                PvMeta {
                    name: "pv0".into(),
                    uuid: "pv0-uuid".into(),
                    pe_start: 0,
                    pe_count: 100,
                },
                PvMeta {
                    name: "pv1".into(),
                    uuid: "pv1-uuid".into(),
                    pe_start: 0,
                    pe_count: 100,
                },
            ],
            logical_volumes: Vec::new(),
        };
        let lv = LvMeta {
            name: "striped_lv".into(),
            uuid: "lv-uuid".into(),
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 2,
                seg_type: SegmentType::Striped {
                    stripe_count: 2,
                    stripe_size: 2,
                },
                stripes: vec![("pv0".into(), 0), ("pv1".into(), 10)],
                areas: vec![pv_area("pv0", 0), pv_area("pv1", 10)],
                dependencies: SegmentDependencies::default(),
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 0), ("pv1".into(), 1_000_000)];

        let map = build_extent_map(&vg, &lv, &pv_offsets).unwrap();

        assert_eq!(map.len(), 8);
        assert_eq!(map[0].logical_start, 0);
        assert_eq!(map[0].pv_index, 0);
        assert_eq!(map[0].physical_offset, 0);
        assert_eq!(map[0].length, 1024);
        assert_eq!(map[1].logical_start, 1024);
        assert_eq!(map[1].pv_index, 1);
        assert_eq!(map[1].physical_offset, 1_000_000 + 10 * 4096);
        assert_eq!(map[2].logical_start, 2048);
        assert_eq!(map[2].pv_index, 0);
        assert_eq!(map[2].physical_offset, 1024);
    }

    #[test]
    fn multi_segment_linear() {
        // Two contiguous segments: first 5 LEs on PV0, next 3 LEs also on PV0
        let vg = make_test_vg();
        let lv = LvMeta {
            name: "multi_seg".into(),
            uuid: "lv-uuid".into(),
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![
                SegmentMeta {
                    start_extent: 0,
                    extent_count: 5,
                    seg_type: SegmentType::Linear,
                    stripes: vec![("pv0".into(), 0)],
                    areas: vec![pv_area("pv0", 0)],
                    dependencies: SegmentDependencies::default(),
                },
                SegmentMeta {
                    start_extent: 5,
                    extent_count: 3,
                    seg_type: SegmentType::Linear,
                    stripes: vec![("pv0".into(), 128)],
                    areas: vec![pv_area("pv0", 128)],
                    dependencies: SegmentDependencies::default(),
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
    fn unsupported_gap_segment_does_not_build_partial_map() {
        let vg = make_test_vg();
        let lv = LvMeta {
            name: "gapped".into(),
            uuid: "lv-uuid".into(),
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 7,
                seg_type: SegmentType::Unsupported {
                    type_name: "gap in segment layout".into(),
                },
                stripes: Vec::new(),
                areas: Vec::new(),
                dependencies: SegmentDependencies::default(),
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512)];

        let err = build_extent_map(&vg, &lv, &pv_offsets).unwrap_err();
        assert!(matches!(err, LvmError::UnsupportedSegment { .. }));
    }

    #[test]
    fn raid0_fails_closed_instead_of_mapping_as_striped() {
        let vg = make_test_vg();
        let lv = LvMeta {
            name: "raid0_lv".into(),
            uuid: "lv-uuid".into(),
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 2,
                seg_type: SegmentType::Raid0 {
                    stripe_count: 2,
                    stripe_size: 8192,
                },
                stripes: vec![("pv0".into(), 0), ("pv0".into(), 100)],
                areas: vec![pv_area("pv0", 0), pv_area("pv0", 100)],
                dependencies: SegmentDependencies::default(),
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512)];
        let err = build_extent_map(&vg, &lv, &pv_offsets).unwrap_err();
        assert!(matches!(
            err,
            LvmError::UnsupportedSegment { seg_type, .. } if seg_type.contains("raid0")
        ));
    }

    #[test]
    fn raid1_fails_closed_until_component_lv_graph_is_supported() {
        let vg = VolumeGroup {
            name: "mirror_vg".into(),
            id: "vg-id".into(),
            extent_size: 4096,
            seqno: 1,
            physical_volumes: vec![
                PvMeta {
                    name: "pv0".into(),
                    uuid: "pv0-uuid".into(),
                    pe_start: 2048,
                    pe_count: 1000,
                },
                PvMeta {
                    name: "pv1".into(),
                    uuid: "pv1-uuid".into(),
                    pe_start: 2048,
                    pe_count: 1000,
                },
            ],
            logical_volumes: Vec::new(),
        };
        let lv = LvMeta {
            name: "mirrored_lv".into(),
            uuid: "lv-uuid".into(),
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 10,
                seg_type: SegmentType::Raid1 { mirror_count: 2 },
                stripes: vec![("pv0".into(), 0), ("pv1".into(), 0)],
                areas: vec![pv_area("pv0", 0), pv_area("pv1", 0)],
                dependencies: SegmentDependencies::default(),
            }],
            size_bytes: 0,
        };
        let pv_offsets = vec![("pv0".into(), 2048 * 512), ("pv1".into(), 4096 * 512)];

        let err = build_extent_map(&vg, &lv, &pv_offsets).unwrap_err();
        assert!(matches!(
            err,
            LvmError::UnsupportedSegment { seg_type, .. }
                if seg_type.contains("component LV graph")
        ));
    }

    #[test]
    fn component_lv_chain_reports_unsupported_dependency_path() {
        let component = LvMeta {
            name: "cache_component".into(),
            uuid: "component-uuid".into(),
            status: Vec::new(),
            role: crate::metadata::LvRole::CacheVolume,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 1,
                seg_type: SegmentType::CacheVolume,
                stripes: Vec::new(),
                areas: vec![
                    SegmentArea::LogicalVolume {
                        name: "cache_pool".into(),
                        start_extent: 0,
                    },
                    SegmentArea::LogicalVolume {
                        name: "origin".into(),
                        start_extent: 0,
                    },
                ],
                dependencies: SegmentDependencies {
                    cache_pool: Some("cache_pool".into()),
                    origin: Some("origin".into()),
                    ..SegmentDependencies::default()
                },
            }],
            size_bytes: 0,
        };
        let lv = LvMeta {
            name: "component_backed".into(),
            uuid: "lv-uuid".into(),
            status: Vec::new(),
            role: crate::metadata::LvRole::Public,
            segments: vec![SegmentMeta {
                start_extent: 0,
                extent_count: 1,
                seg_type: SegmentType::Linear,
                stripes: Vec::new(),
                areas: vec![SegmentArea::LogicalVolume {
                    name: "cache_component".into(),
                    start_extent: 0,
                }],
                dependencies: SegmentDependencies::default(),
            }],
            size_bytes: 0,
        };
        let mut vg = make_test_vg();
        vg.logical_volumes = vec![component, lv.clone()];
        let pv_offsets = vec![("pv0".into(), 2048 * 512)];

        let err = build_extent_map(&vg, &lv, &pv_offsets).unwrap_err();

        assert!(matches!(
            err,
            LvmError::UnsupportedSegment { lv_name, seg_type }
                if lv_name == "cache_component"
                    && seg_type.contains("cache")
                    && seg_type.contains("component_backed -> cache_component")
        ));
    }
}
