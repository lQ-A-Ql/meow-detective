use ceph_wire::{
    decode_cephfs_file_layout, format_cephfs_data_object_name, CephFsFileLayout, CephWireError,
};

const KIB: u32 = 1024;

#[test]
fn decodes_v2_and_legacy_layouts_with_the_upstream_empty_layout_rule() {
    let v2 = v2_layout(64 * KIB, 2, 128 * KIB, 7, "data");
    let decoded = decode_cephfs_file_layout(&v2).unwrap();
    assert_eq!(decoded.stripe_unit, 64 * KIB);
    assert_eq!(decoded.stripe_count, 2);
    assert_eq!(decoded.object_size, 128 * KIB);
    assert_eq!(decoded.pool_id, 7);
    assert_eq!(decoded.pool_namespace, "data");

    let legacy = legacy_layout(0, 0, 0, 0);
    let decoded = decode_cephfs_file_layout(&legacy).unwrap();
    assert!(decoded.is_empty());
    assert_eq!(decoded.pool_id, -1);
}

#[test]
fn plans_striped_ranges_without_crossing_units_or_objects() {
    let layout = CephFsFileLayout::new(64 * KIB, 2, 128 * KIB, 7, "").unwrap();
    let segments = layout
        .plan_range(512 * KIB as u64, 0, (320 * KIB) as usize)
        .unwrap();
    assert_eq!(segments[0].object_number, 0);
    assert_eq!(segments[0].object_offset, 0);
    assert_eq!(segments[0].length, 64 * KIB as u64);
    assert_eq!(segments[1].object_number, 1);
    assert_eq!(segments[2].object_number, 0);
    assert_eq!(segments[2].object_offset, 64 * KIB as u64);
    assert_eq!(segments[4].object_number, 2);
    assert_eq!(segments.last().unwrap().logical_offset, 256 * KIB as u64);
}

#[test]
fn rejects_invalid_layouts_and_ranges() {
    assert!(matches!(
        CephFsFileLayout::new(4 * KIB, 1, 4 * KIB, 1, ""),
        Err(CephWireError::InvalidCephFsLayout {
            field: "stripe_unit",
            ..
        })
    ));
    let layout = CephFsFileLayout::new(64 * KIB, 1, 64 * KIB, 1, "").unwrap();
    assert!(matches!(
        layout.plan_range(64, 0, 65),
        Err(CephWireError::CephFsLayoutRangeOutOfBounds { .. })
    ));
    assert!(matches!(
        layout.plan_range(u64::MAX, u64::MAX, 1),
        Err(CephWireError::CephFsLayoutRangeOverflow)
    ));
    assert_eq!(
        format_cephfs_data_object_name(0x123, 4).unwrap(),
        "123.00000004"
    );
}

fn v2_layout(
    stripe_unit: u32,
    stripe_count: u32,
    object_size: u32,
    pool: i64,
    namespace: &str,
) -> Vec<u8> {
    let mut payload = Vec::new();
    put_u32(&mut payload, stripe_unit);
    put_u32(&mut payload, stripe_count);
    put_u32(&mut payload, object_size);
    put_i64(&mut payload, pool);
    put_string(&mut payload, namespace);
    let mut output = vec![2, 2];
    put_u32(&mut output, payload.len() as u32);
    output.extend(payload);
    output
}

fn legacy_layout(stripe_unit: u32, stripe_count: u32, object_size: u32, pool: u32) -> Vec<u8> {
    let mut output = Vec::new();
    for value in [stripe_unit, stripe_count, object_size, 0, 0, 0, pool] {
        put_u32(&mut output, value);
    }
    output
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend(value.to_le_bytes());
}

fn put_i64(output: &mut Vec<u8>, value: i64) {
    output.extend(value.to_le_bytes());
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    put_u32(output, value.len() as u32);
    output.extend(value.as_bytes());
}
