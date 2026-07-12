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
        extent_size: 8192,
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

fn linear_lv(name: &str, extent_count: u64, start_extent: u64) -> LvMeta {
    LvMeta {
        name: name.into(),
        uuid: format!("{name}-uuid"),
        status: Vec::new(),
        role: crate::metadata::LvRole::Public,
        segments: vec![SegmentMeta {
            start_extent: 0,
            extent_count,
            seg_type: SegmentType::Linear,
            stripes: vec![("pv0".into(), start_extent)],
            areas: vec![pv_area("pv0", start_extent)],
            dependencies: SegmentDependencies::default(),
        }],
        size_bytes: 0,
    }
}

#[test]
fn linear_single_pv_mapping() {
    let vg = make_test_vg();
    let lv = linear_lv("root", 10, 0);
    let map = build_extent_map(&vg, &lv, &[("pv0".into(), 2048 * 512)]).unwrap();
    assert_eq!(map.len(), 1);
    assert_eq!(map[0].logical_start, 0);
    assert_eq!(map[0].physical_offset, 2048 * 512);
    assert_eq!(map[0].length, 10 * 8192 * 512);
    assert_eq!(map[0].pv_index, 0);
}

#[test]
fn linear_with_offset() {
    let vg = make_test_vg();
    let lv = linear_lv("home", 5, 1280);
    let map = build_extent_map(&vg, &lv, &[("pv0".into(), 2048 * 512)]).unwrap();
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
    let error = build_extent_map(&vg, &lv, &[("pv0".into(), 2048 * 512)]).unwrap_err();
    assert!(error.to_string().contains("thin-pool"));
}

#[test]
fn striped_two_pv_mapping() {
    let vg = two_pv_vg("test_vg", 8192);
    let lv = LvMeta {
        name: "striped_lv".into(),
        uuid: "lv-uuid".into(),
        status: Vec::new(),
        role: crate::metadata::LvRole::Public,
        segments: vec![SegmentMeta {
            start_extent: 0,
            extent_count: 4,
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
    let map = build_extent_map(
        &vg,
        &lv,
        &[("pv0".into(), 2048 * 512), ("pv1".into(), 2048 * 512)],
    )
    .unwrap();
    assert_eq!(map.len(), 4);
    assert_eq!((map[0].logical_start, map[0].pv_index), (0, 0));
    assert_eq!((map[1].logical_start, map[1].pv_index), (8192 * 512, 1));
    assert_eq!((map[2].logical_start, map[2].pv_index), (2 * 8192 * 512, 0));
    assert_eq!((map[3].logical_start, map[3].pv_index), (3 * 8192 * 512, 1));
    assert_eq!(map[2].physical_offset, 2048 * 512 + 8192 * 512);
    assert_eq!(map[3].physical_offset, 2048 * 512 + 8192 * 512);
}

#[test]
fn striped_mapping_uses_stripe_size_chunks() {
    let vg = two_pv_vg("chunk_vg", 8);
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
    let map = build_extent_map(&vg, &lv, &[("pv0".into(), 0), ("pv1".into(), 1_000_000)]).unwrap();
    assert_eq!(map.len(), 8);
    assert_eq!((map[0].logical_start, map[0].pv_index), (0, 0));
    assert_eq!(map[0].physical_offset, 0);
    assert_eq!(map[0].length, 1024);
    assert_eq!((map[1].logical_start, map[1].pv_index), (1024, 1));
    assert_eq!(map[1].physical_offset, 1_000_000 + 10 * 4096);
    assert_eq!(map[2].physical_offset, 1024);
}

#[test]
fn multi_segment_linear() {
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
    let map = build_extent_map(&vg, &lv, &[("pv0".into(), 2048 * 512)]).unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map[0].length, 5 * 8192 * 512);
    assert_eq!(map[1].logical_start, 5 * 8192 * 512);
    assert_eq!(map[1].physical_offset, 2048 * 512 + 128 * 8192 * 512);
    assert_eq!(map[1].length, 3 * 8192 * 512);
}

#[test]
fn unsupported_gap_segment_does_not_build_partial_map() {
    let vg = make_test_vg();
    let lv = unsupported_lv("gapped", "gap in segment layout");
    let error = build_extent_map(&vg, &lv, &[("pv0".into(), 2048 * 512)]).unwrap_err();
    assert!(matches!(error, LvmError::UnsupportedSegment { .. }));
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
    let error = build_extent_map(&vg, &lv, &[("pv0".into(), 2048 * 512)]).unwrap_err();
    assert!(matches!(
        error,
        LvmError::UnsupportedSegment { seg_type, .. } if seg_type.contains("raid0")
    ));
}

#[test]
fn raid1_fails_closed_until_component_lv_graph_is_supported() {
    let vg = two_pv_vg("mirror_vg", 4096);
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
    let error = build_extent_map(
        &vg,
        &lv,
        &[("pv0".into(), 2048 * 512), ("pv1".into(), 4096 * 512)],
    )
    .unwrap_err();
    assert!(matches!(
        error,
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
    let error = build_extent_map(&vg, &lv, &[("pv0".into(), 2048 * 512)]).unwrap_err();
    assert!(matches!(
        error,
        LvmError::UnsupportedSegment { lv_name, seg_type }
            if lv_name == "cache_component"
                && seg_type.contains("cache")
                && seg_type.contains("component_backed -> cache_component")
    ));
}

fn two_pv_vg(name: &str, extent_size: u64) -> VolumeGroup {
    VolumeGroup {
        name: name.into(),
        id: "vg-id".into(),
        extent_size,
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
    }
}

fn unsupported_lv(name: &str, reason: &str) -> LvMeta {
    LvMeta {
        name: name.into(),
        uuid: "lv-uuid".into(),
        status: Vec::new(),
        role: crate::metadata::LvRole::Public,
        segments: vec![SegmentMeta {
            start_extent: 0,
            extent_count: 7,
            seg_type: SegmentType::Unsupported {
                type_name: reason.into(),
            },
            stripes: Vec::new(),
            areas: Vec::new(),
            dependencies: SegmentDependencies::default(),
        }],
        size_bytes: 0,
    }
}
