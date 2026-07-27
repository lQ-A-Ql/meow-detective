use super::*;

#[test]
fn formats_mixed_endian_guid() {
    // On-disk bytes of the BitLocker To Go identifier in the public bdetogo.raw
    // sample at offset 424. The first three fields are little-endian and the last
    // eight bytes big-endian, so the rendered form is not a straight hex dump.
    let raw = [
        0x3b, 0xd6, 0x67, 0x49, 0x29, 0x2e, 0xd8, 0x4a, 0x83, 0x99, 0xf6, 0xa3, 0x39, 0xe3, 0xd0,
        0x01,
    ];
    assert_eq!(format_guid(&raw), "4967d63b-2e29-4ad8-8399-f6a339e3d001");
}

#[test]
fn formats_the_nil_guid() {
    assert_eq!(
        format_guid(&[0u8; 16]),
        "00000000-0000-0000-0000-000000000000"
    );
}

#[test]
fn renders_canonical_shape_and_lowercase_hex() {
    let raw = [0xFFu8; 16];
    let rendered = format_guid(&raw);
    assert_eq!(rendered, "ffffffff-ffff-ffff-ffff-ffffffffffff");
    let groups: Vec<usize> = rendered.split('-').map(str::len).collect();
    assert_eq!(groups, vec![8, 4, 4, 4, 12]);
}
