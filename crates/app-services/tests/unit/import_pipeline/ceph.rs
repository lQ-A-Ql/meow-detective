use std::collections::BTreeMap;

fn bluefs_super(
    extents: Vec<ceph_wire::BluefsExtent>,
    layout: Option<ceph_wire::BluefsLayout>,
) -> ceph_wire::BluefsSuper {
    ceph_wire::BluefsSuper {
        uuid: uuid::Uuid::new_v4(),
        osd_uuid: uuid::Uuid::new_v4(),
        seq: 50,
        block_size: 4096,
        log_fnode: ceph_wire::BluefsFnode {
            ino: 1,
            size: 0,
            mtime: ceph_wire::CephUtime {
                seconds: 1,
                nanoseconds: 0,
            },
            extents,
            encoding: 0,
            content_size: 0,
            struct_version: 1,
            struct_compat_version: 1,
        },
        memorized_layout: layout,
        crc32c: 0,
        struct_version: 2,
        struct_compat_version: 1,
    }
}

fn bluefs_extent(offset: u64, length: u32, bdev: u8) -> ceph_wire::BluefsExtent {
    ceph_wire::BluefsExtent {
        offset,
        length,
        bdev,
        struct_version: 1,
        struct_compat_version: 1,
    }
}

fn shared_layout() -> ceph_wire::BluefsLayout {
    ceph_wire::BluefsLayout {
        shared_bdev: 1,
        dedicated_db: false,
        dedicated_wal: false,
        struct_version: 1,
        struct_compat_version: 1,
    }
}

#[test]
fn sanitized_metadata_removes_secret_and_records_presence() {
    let metadata = BTreeMap::from([
        ("osd_key".to_string(), "secret".to_string()),
        ("whoami".to_string(), "2".to_string()),
        ("future_credential".to_string(), "also-secret".to_string()),
    ]);

    let sanitized = super::sanitized_metadata(&metadata, true);

    assert!(!sanitized.contains_key("osd_key"));
    assert_eq!(
        sanitized.get("osd_key_present").map(String::as_str),
        Some("true")
    );
    assert_eq!(sanitized.get("whoami").map(String::as_str), Some("2"));
    assert!(!sanitized.contains_key("future_credential"));
    let json = serde_json::to_string(&sanitized).unwrap();
    assert!(!json.contains("secret"));
}

#[test]
fn boolean_metadata_uses_ceph_truthy_values() {
    assert_eq!(
        super::parse_bool(Some(&"1".to_string())).unwrap(),
        Some(true)
    );
    assert_eq!(
        super::parse_bool(Some(&"yes".to_string())).unwrap(),
        Some(true)
    );
    assert_eq!(
        super::parse_bool(Some(&"0".to_string())).unwrap(),
        Some(false)
    );
    assert_eq!(super::parse_bool(None).unwrap(), None);
    assert!(super::parse_bool(Some(&"maybe".to_string())).is_err());
}

#[test]
fn label_health_distinguishes_selected_stale_and_single_replicas() {
    use ceph_wire::{BdevLabel, BdevLabelSelection, CephUtime};
    use uuid::Uuid;

    let osd_uuid = Uuid::new_v4();
    let label = BdevLabel {
        osd_uuid,
        size: 4096,
        birth_time: CephUtime {
            seconds: 1,
            nanoseconds: 0,
        },
        description: "main".to_string(),
        metadata: BTreeMap::from([
            ("multi".to_string(), "yes".to_string()),
            ("epoch".to_string(), "2".to_string()),
        ]),
        struct_version: 2,
        struct_compat_version: 1,
    };
    let replicas = vec![
        super::LabelReplica {
            position: 0,
            label: label.clone(),
        },
        super::LabelReplica {
            position: 1 << 30,
            label: label.clone(),
        },
    ];
    let healthy = BdevLabelSelection {
        label: label.clone(),
        valid_positions: vec![0, 1 << 30],
        is_multi: true,
        epoch: Some(2),
    };
    assert_eq!(super::label_health(&replicas, &healthy), "healthy");

    let stale = BdevLabelSelection {
        valid_positions: vec![1 << 30],
        ..healthy
    };
    assert_eq!(super::label_health(&replicas, &stale), "staleReplica");
    assert_eq!(super::label_health(&replicas[..1], &stale), "singleReplica");
}

#[test]
fn bluefs_extent_validation_accepts_bound_shared_device_ranges() {
    let superblock = bluefs_super(vec![bluefs_extent(8192, 65_536, 1)], Some(shared_layout()));

    super::validate_bluefs_extents(&superblock, 1024 * 1024)
        .expect("shared extent must be accepted");
}

#[test]
fn bluefs_extent_validation_rejects_unknown_devices_and_out_of_bounds_ranges() {
    let unknown_device = bluefs_super(vec![bluefs_extent(4096, 65_536, 2)], Some(shared_layout()));
    assert!(super::validate_bluefs_extents(&unknown_device, 1024 * 1024).is_err());

    let out_of_bounds = bluefs_super(
        vec![bluefs_extent(1024 * 1024 - 4096, 65_536, 1)],
        Some(shared_layout()),
    );
    assert!(super::validate_bluefs_extents(&out_of_bounds, 1024 * 1024).is_err());

    let unaligned = bluefs_super(vec![bluefs_extent(4097, 65_536, 1)], Some(shared_layout()));
    assert!(super::validate_bluefs_extents(&unaligned, 1024 * 1024).is_err());
}

#[test]
fn bluefs_extent_validation_rejects_reserved_region_and_unknown_shared_device() {
    let reserved = bluefs_super(vec![bluefs_extent(4096, 4096, 1)], Some(shared_layout()));
    assert!(super::validate_bluefs_extents(&reserved, 1024 * 1024).is_err());

    let mut invalid_layout = shared_layout();
    invalid_layout.shared_bdev = 255;
    let invalid_device = bluefs_super(vec![bluefs_extent(8192, 65_536, 255)], Some(invalid_layout));
    assert!(super::validate_bluefs_extents(&invalid_device, 1024 * 1024).is_err());
}

#[test]
fn bluefs_extent_validation_rejects_ambiguous_legacy_layout() {
    let superblock = bluefs_super(vec![bluefs_extent(4096, 65_536, 1)], None);

    assert!(super::validate_bluefs_extents(&superblock, 1024 * 1024).is_err());
}

#[test]
fn bluefs_extent_validation_rejects_dedicated_device_layouts() {
    let mut layout = shared_layout();
    layout.dedicated_db = true;
    let superblock = bluefs_super(vec![bluefs_extent(4096, 65_536, 1)], Some(layout));

    assert!(super::validate_bluefs_extents(&superblock, 1024 * 1024).is_err());
}
