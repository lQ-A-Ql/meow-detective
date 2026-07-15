use ceph_wire::{
    decode_rbd_data_pool_id, decode_rbd_features, decode_rbd_id, decode_rbd_name,
    decode_rbd_object_prefix, decode_rbd_order, decode_rbd_size, decode_rbd_string,
    decode_rbd_stripe_count, decode_rbd_stripe_unit, format_rbd_data_object_name, CephEncode,
    CephWireError, RbdHeadImageLayout, RbdImageMetadata,
};

fn encode<T: CephEncode>(value: T) -> Vec<u8> {
    let mut bytes = Vec::new();
    value.encode(&mut bytes);
    bytes
}

fn layout(
    image_size: u64,
    order: u8,
    prefix: &str,
    stripe_unit: u64,
    stripe_count: u64,
) -> RbdHeadImageLayout {
    RbdHeadImageLayout::new(
        image_size,
        order,
        prefix.to_string(),
        stripe_unit,
        stripe_count,
    )
    .expect("valid layout")
}

#[test]
fn decodes_all_normalized_rbd_metadata_primitives() {
    assert_eq!(decode_rbd_order(&encode(12u8)).unwrap(), 12);
    assert_eq!(
        decode_rbd_size(&encode(0x0102_0304u64)).unwrap(),
        0x0102_0304
    );
    assert_eq!(
        decode_rbd_features(&encode(0x1122_3344u64)).unwrap(),
        0x1122_3344
    );
    assert_eq!(decode_rbd_stripe_unit(&encode(4096u64)).unwrap(), 4096);
    assert_eq!(decode_rbd_stripe_count(&encode(2u64)).unwrap(), 2);
    assert_eq!(decode_rbd_data_pool_id(&encode(-7i64)).unwrap(), -7);

    assert_eq!(
        decode_rbd_object_prefix(&encode("rbd_data.1.ab".to_string())).unwrap(),
        "rbd_data.1.ab"
    );
    assert_eq!(
        decode_rbd_name(&encode("vm-disk".to_string())).unwrap(),
        "vm-disk"
    );
    assert_eq!(
        decode_rbd_id(&encode("abc123".to_string())).unwrap(),
        "abc123"
    );
}

#[test]
fn decodes_generic_rbd_strings_and_rejects_trailing_bytes() {
    let mut bytes = encode("value".to_string());
    bytes.push(0);
    assert_eq!(
        decode_rbd_string(&encode("value".to_string()), "test").unwrap(),
        "value"
    );
    assert_eq!(
        decode_rbd_string(&bytes, "test").unwrap_err(),
        CephWireError::RbdTrailingBytes {
            field: "test",
            remaining: 1
        }
    );
}

#[test]
fn rejects_truncated_invalid_and_oversized_metadata() {
    assert!(matches!(
        decode_rbd_size(&[1, 2, 3]),
        Err(CephWireError::UnexpectedEof { .. })
    ));
    assert!(matches!(
        decode_rbd_order(&encode(11u8)),
        Err(CephWireError::InvalidRbdMetadata { field: "order", .. })
    ));
    assert!(matches!(
        decode_rbd_order(&encode(26u8)),
        Err(CephWireError::InvalidRbdMetadata { field: "order", .. })
    ));

    let oversized_name = "n".repeat(97);
    assert!(matches!(
        decode_rbd_name(&encode(oversized_name)),
        Err(CephWireError::LengthLimit {
            context: "name",
            ..
        })
    ));
    assert!(matches!(
        decode_rbd_id(&[1, 0, 0, 0, 0xff]),
        Err(CephWireError::InvalidUtf8 { context: "id", .. })
    ));
    assert!(matches!(
        decode_rbd_id(&encode(String::new())),
        Err(CephWireError::InvalidRbdMetadata { field: "id", .. })
    ));
    assert!(matches!(
        decode_rbd_id(&encode("bad-id".to_string())),
        Err(CephWireError::InvalidRbdMetadata { field: "id", .. })
    ));
    assert!(matches!(
        decode_rbd_object_prefix(&encode("bad\0prefix".to_string())),
        Err(CephWireError::InvalidRbdMetadata {
            field: "object_prefix",
            ..
        })
    ));
}

#[test]
fn default_striping_normalizes_to_one_object_per_object_size() {
    let layout = layout(9000, 12, "rbd_data.1.img", 0, 0);
    assert_eq!(layout.object_size, 4096);
    assert_eq!(layout.stripe_unit, 4096);
    assert_eq!(layout.stripe_count, 1);

    let plans = layout.map_range(0, 9000).unwrap();
    assert_eq!(plans.len(), 3);
    assert_eq!(plans[0].object_no, 0);
    assert_eq!(plans[0].object_offset, 0);
    assert_eq!(plans[0].length, 4096);
    assert_eq!(plans[0].destination_offset, 0);
    assert_eq!(plans[1].object_no, 1);
    assert_eq!(plans[1].length, 4096);
    assert_eq!(plans[2].object_no, 2);
    assert_eq!(plans[2].length, 808);
}

#[test]
fn striped_layout_maps_each_logical_chunk_to_ceph_object_coordinates() {
    let layout = layout(8192, 12, "rbd_data.1.img", 1024, 2);
    let plans = layout.map_range(0, 8192).unwrap();
    let expected = [
        (0, 0, 1024, 0),
        (1, 0, 1024, 1024),
        (0, 1024, 1024, 2048),
        (1, 1024, 1024, 3072),
        (0, 2048, 1024, 4096),
        (1, 2048, 1024, 5120),
        (0, 3072, 1024, 6144),
        (1, 3072, 1024, 7168),
    ];

    assert_eq!(plans.len(), expected.len());
    for (plan, (object_no, object_offset, length, destination_offset)) in plans.iter().zip(expected)
    {
        assert_eq!(
            (
                plan.object_no,
                plan.object_offset,
                plan.length,
                plan.destination_offset
            ),
            (object_no, object_offset, length, destination_offset)
        );
    }
}

#[test]
fn maps_object_boundary_and_tail_ranges() {
    let layout = layout(5000, 12, "rbd_data.1.img", 0, 0);
    let plans = layout.map_range(4090, 1000).unwrap();
    assert_eq!(plans.len(), 2);
    assert_eq!(
        (
            plans[0].object_no,
            plans[0].object_offset,
            plans[0].length,
            plans[0].destination_offset
        ),
        (0, 4090, 6, 0)
    );
    assert_eq!(
        (
            plans[1].object_no,
            plans[1].object_offset,
            plans[1].length,
            plans[1].destination_offset
        ),
        (1, 0, 904, 6)
    );
}

#[test]
fn clips_ranges_at_image_end_and_handles_empty_reads() {
    let layout = layout(4096, 12, "rbd_data.1.img", 0, 0);
    assert!(layout.map_range(0, 0).unwrap().is_empty());
    assert_eq!(layout.map_range(4095, 100).unwrap()[0].length, 1);
    assert!(matches!(
        layout.map_range(4096, 1),
        Err(CephWireError::RbdRangeOutOfBounds {
            offset: 4096,
            length: 1,
            image_size: 4096
        })
    ));
}

#[test]
fn rejects_layout_invariants_and_range_overflow() {
    for (order, unit, count) in [(11, 0, 0), (26, 0, 0), (12, 0, 1), (12, 1024, 0)] {
        assert!(matches!(
            RbdHeadImageLayout::new(4096, order, "prefix", unit, count),
            Err(CephWireError::InvalidRbdLayout { .. })
        ));
    }
    assert!(matches!(
        RbdHeadImageLayout::new(4096, 12, "prefix", 1000, 1),
        Err(CephWireError::InvalidRbdLayout { .. })
    ));
    assert!(matches!(
        RbdHeadImageLayout::new(4096, 12, "", 0, 0),
        Err(CephWireError::InvalidRbdLayout { .. })
    ));
    assert!(matches!(
        RbdHeadImageLayout::new(4096, 12, "prefix", 4096, u64::MAX),
        Err(CephWireError::InvalidRbdLayout { .. })
    ));
    let layout = layout(4096, 12, "prefix", 0, 0);
    assert!(matches!(
        layout.map_range(u64::MAX, 1),
        Err(CephWireError::RbdRangeOverflow { .. })
    ));
}

#[test]
fn formats_canonical_v2_data_object_names() {
    assert_eq!(
        format_rbd_data_object_name("rbd_data.1.img", 0xabu64).unwrap(),
        "rbd_data.1.img.00000000000000ab"
    );
    let layout = layout(4096, 12, "rbd_data.1.img", 0, 0);
    assert_eq!(
        layout.data_object_name(42),
        "rbd_data.1.img.000000000000002a"
    );
    assert_eq!(
        layout.map_range(0, 1).unwrap()[0]
            .data_object_name("rbd_data.1.img")
            .unwrap(),
        "rbd_data.1.img.0000000000000000"
    );
}

#[test]
fn builds_layout_from_normalized_metadata() {
    let metadata = RbdImageMetadata {
        name: "vm-disk".to_string(),
        id: "abc123".to_string(),
        object_prefix: "rbd_data.1.abc123".to_string(),
        image_size: 8192,
        order: 12,
        features: 0x11,
        stripe_unit: 1024,
        stripe_count: 2,
        data_pool_id: -1,
    };
    let layout = RbdHeadImageLayout::from_metadata(&metadata).unwrap();
    assert_eq!(layout.features, 0x11);
    assert_eq!(layout.stripe_unit, 1024);
    assert_eq!(layout.stripe_count, 2);
}
