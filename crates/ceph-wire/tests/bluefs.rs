use ceph_wire::{
    crc32c::ceph_crc32c, decode_bluefs_super_block, decode_lba_u64, decode_varint_lowz_u64,
    decode_varint_u64, CephCursor, CephEncode, CephStructEnvelope, CephWireError,
    BLUEFS_SUPER_BLOCK_SIZE, BLUEFS_SUPER_OFFSET,
};
use uuid::Uuid;

const SERVER01_SUPER_PREFIX_HEX: &str = concat!(
    "02015a000000394d12df402344dcb4c510b5e5dd48f49630c2a5650a4395a47a",
    "ec496515bd6132000000000000000010000001011b0000000100bd28ea6978daf9",
    "2500010000000101060000001ca202004301010101060000000100000000004cb1",
    "311b",
);

const SERVER02_SUPER_PREFIX_HEX: &str = concat!(
    "02015a000000e1b8a63e3c9347438232b236b82fec83de8554def932448dbe2c",
    "0474df6c16c532000000000000000010000001011b0000000100be28ea69b8c1",
    "652e00010000000101060000006ca00200430101010106000000010000000000",
    "72c4d517",
);

const SERVER03_SUPER_PREFIX_HEX: &str = concat!(
    "02015a000000d8f0162eaefe4397ad6416b28af988a1cd6f9b5c37d54dc085889",
    "669d156b02c32000000000000000010000001011b0000000100be28ea691dd9dc1",
    "b0001000000010106000000d429010043010101010600000001000000000045a6",
    "3878",
);

fn superblock_from_prefix(prefix: &str) -> Vec<u8> {
    let mut block = vec![0; BLUEFS_SUPER_BLOCK_SIZE];
    for (index, pair) in prefix.as_bytes().chunks_exact(2).enumerate() {
        let digits = std::str::from_utf8(pair).expect("ASCII fixture hex");
        block[index] = u8::from_str_radix(digits, 16).expect("valid fixture hex");
    }
    block
}

fn server01_superblock() -> Vec<u8> {
    superblock_from_prefix(SERVER01_SUPER_PREFIX_HEX)
}

fn append_envelope(version: u8, compat_version: u8, payload: &[u8], output: &mut Vec<u8>) {
    CephStructEnvelope {
        version,
        compat_version,
        payload_length: payload.len() as u32,
    }
    .encode(output);
    output.extend_from_slice(payload);
}

fn envelope_mode_superblock() -> Vec<u8> {
    let mut fnode_payload = vec![7, 0xd2, 0x09];
    1_700_000_000u32.encode(&mut fnode_payload);
    123_456_789u32.encode(&mut fnode_payload);
    0u8.encode(&mut fnode_payload);
    0u32.encode(&mut fnode_payload);
    1u8.encode(&mut fnode_payload);
    fnode_payload.extend_from_slice(&[0xe8, 0x07]);

    let mut fnode = Vec::new();
    append_envelope(2, 2, &fnode_payload, &mut fnode);

    let mut super_payload = Vec::new();
    Uuid::parse_str("11111111-2222-4333-8444-555555555555")
        .unwrap()
        .encode(&mut super_payload);
    Uuid::parse_str("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee")
        .unwrap()
        .encode(&mut super_payload);
    99u64.encode(&mut super_payload);
    4096u32.encode(&mut super_payload);
    super_payload.extend_from_slice(&fnode);
    0u8.encode(&mut super_payload);

    let mut block = Vec::new();
    append_envelope(3, 3, &super_payload, &mut block);
    let crc = ceph_crc32c(&block);
    crc.encode(&mut block);
    block.resize(BLUEFS_SUPER_BLOCK_SIZE, 0);
    block
}

#[test]
fn decodes_server01_bluefs_superblock_oracle() {
    assert_eq!(BLUEFS_SUPER_OFFSET, 4096);
    let superblock = decode_bluefs_super_block(&server01_superblock()).unwrap();

    assert_eq!(
        superblock.uuid,
        Uuid::parse_str("394d12df-4023-44dc-b4c5-10b5e5dd48f4").unwrap()
    );
    assert_eq!(
        superblock.osd_uuid,
        Uuid::parse_str("9630c2a5-650a-4395-a47a-ec496515bd61").unwrap()
    );
    assert_eq!(superblock.seq, 50);
    assert_eq!(superblock.block_size, 4096);
    assert_eq!(superblock.crc32c, 0x1b31_b14c);
    assert_eq!(superblock.struct_version, 2);
    assert_eq!(superblock.struct_compat_version, 1);

    let fnode = &superblock.log_fnode;
    assert_eq!(fnode.ino, 1);
    assert_eq!(fnode.size, 0);
    assert_eq!(fnode.mtime.seconds, 1_776_953_533);
    assert_eq!(fnode.mtime.nanoseconds, 637_131_384);
    assert_eq!(fnode.encoding, 0);
    assert_eq!(fnode.content_size, 0);
    assert_eq!(fnode.extents.len(), 1);
    assert_eq!(fnode.extents[0].offset, 353_427_456);
    assert_eq!(fnode.extents[0].length, 65_536);
    assert_eq!(fnode.extents[0].bdev, 1);

    let layout = superblock.memorized_layout.as_ref().unwrap();
    assert_eq!(layout.shared_bdev, 1);
    assert!(!layout.dedicated_db);
    assert!(!layout.dedicated_wal);
}

#[test]
fn decodes_all_three_pve_bluefs_superblock_oracles() {
    let expected = [
        (
            SERVER01_SUPER_PREFIX_HEX,
            "394d12df-4023-44dc-b4c5-10b5e5dd48f4",
            "9630c2a5-650a-4395-a47a-ec496515bd61",
            0x1b31_b14c,
        ),
        (
            SERVER02_SUPER_PREFIX_HEX,
            "e1b8a63e-3c93-4743-8232-b236b82fec83",
            "de8554de-f932-448d-be2c-0474df6c16c5",
            0x17d5_c472,
        ),
        (
            SERVER03_SUPER_PREFIX_HEX,
            "d8f0162e-aefe-4397-ad64-16b28af988a1",
            "cd6f9b5c-37d5-4dc0-8588-9669d156b02c",
            0x7838_a645,
        ),
    ];

    for (prefix, bluefs_uuid, osd_uuid, crc32c) in expected {
        let superblock = decode_bluefs_super_block(&superblock_from_prefix(prefix)).unwrap();
        assert_eq!(superblock.uuid, Uuid::parse_str(bluefs_uuid).unwrap());
        assert_eq!(superblock.osd_uuid, Uuid::parse_str(osd_uuid).unwrap());
        assert_eq!(superblock.seq, 50);
        assert_eq!(superblock.block_size, 4096);
        assert_eq!(superblock.crc32c, crc32c);
        assert_eq!(superblock.log_fnode.extents.len(), 1);
    }
}

#[test]
fn decodes_envelope_mode_fnode_and_absent_layout() {
    let superblock = decode_bluefs_super_block(&envelope_mode_superblock()).unwrap();

    assert_eq!(superblock.struct_version, 3);
    assert_eq!(superblock.struct_compat_version, 3);
    assert_eq!(superblock.seq, 99);
    assert!(superblock.memorized_layout.is_none());
    assert_eq!(superblock.log_fnode.struct_version, 2);
    assert_eq!(superblock.log_fnode.struct_compat_version, 2);
    assert_eq!(superblock.log_fnode.ino, 7);
    assert_eq!(superblock.log_fnode.size, 1234);
    assert_eq!(superblock.log_fnode.encoding, 1);
    assert_eq!(superblock.log_fnode.content_size, 1000);
    assert!(superblock.log_fnode.extents.is_empty());
}

#[test]
fn rejects_bluefs_superblock_crc_mismatch() {
    let mut block = server01_superblock();
    block[6] ^= 0x01;
    assert!(matches!(
        decode_bluefs_super_block(&block),
        Err(CephWireError::BluefsCrcMismatch { .. })
    ));
}

#[test]
fn rejects_unbounded_bluefs_extent_count() {
    let mut block = server01_superblock();
    block[67..71].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode_bluefs_super_block(&block),
        Err(CephWireError::LengthLimit {
            context: "BlueFS fnode extents",
            ..
        })
    ));
}

#[test]
fn rejects_non_block_sized_input() {
    assert_eq!(
        decode_bluefs_super_block(&[0; 32]).unwrap_err(),
        CephWireError::InvalidBluefsSuperblockSize {
            expected: BLUEFS_SUPER_BLOCK_SIZE,
            actual: 32,
        }
    );
}

#[test]
fn rejects_invalid_block_size_and_boolean_wire_values() {
    let mut bad_block_size = server01_superblock();
    bad_block_size[46..50].copy_from_slice(&3072u32.to_le_bytes());
    assert_eq!(
        decode_bluefs_super_block(&bad_block_size).unwrap_err(),
        CephWireError::InvalidBluefsBlockSize { block_size: 3072 }
    );

    let mut bad_layout_presence = server01_superblock();
    bad_layout_presence[83] = 2;
    assert_eq!(
        decode_bluefs_super_block(&bad_layout_presence).unwrap_err(),
        CephWireError::InvalidBluefsBoolean {
            context: "optional layout presence",
            value: 2,
        }
    );

    let mut bad_dedicated_db = server01_superblock();
    bad_dedicated_db[94] = 2;
    assert_eq!(
        decode_bluefs_super_block(&bad_dedicated_db).unwrap_err(),
        CephWireError::InvalidBluefsBoolean {
            context: "dedicated DB",
            value: 2,
        }
    );

    let mut bad_dedicated_wal = server01_superblock();
    bad_dedicated_wal[95] = 2;
    assert_eq!(
        decode_bluefs_super_block(&bad_dedicated_wal).unwrap_err(),
        CephWireError::InvalidBluefsBoolean {
            context: "dedicated WAL",
            value: 2,
        }
    );
}

#[test]
fn rejects_zero_extent_length() {
    let mut block = server01_superblock();
    block[81] = 0;
    assert_eq!(
        decode_bluefs_super_block(&block).unwrap_err(),
        CephWireError::InvalidBluefsExtentLength { length: 0 }
    );
}

#[test]
fn compressed_integer_decoders_are_bounded() {
    let mut varint = CephCursor::new(&[0xff; 10]);
    assert!(matches!(
        decode_varint_u64(&mut varint, "test varint"),
        Err(CephWireError::IntegerOverflow {
            context: "test varint"
        })
    ));

    let mut lowz = CephCursor::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x3f]);
    assert!(matches!(
        decode_varint_lowz_u64(&mut lowz, "test lowz"),
        Err(CephWireError::IntegerOverflow {
            context: "test lowz"
        })
    ));

    let mut lba = CephCursor::new(&[0x00, 0x00, 0x00]);
    assert!(matches!(
        decode_lba_u64(&mut lba, "test LBA"),
        Err(CephWireError::UnexpectedEof { .. })
    ));
}

#[test]
fn varint_and_low_zero_varint_decode_known_ceph_vectors() {
    let vectors: &[(&[u8], u64)] = &[
        (&[0x00], 0),
        (&[0xac, 0x02], 300),
        (
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01],
            u64::MAX,
        ),
    ];
    for (encoded, expected) in vectors {
        let mut cursor = CephCursor::new(encoded);
        assert_eq!(
            decode_varint_u64(&mut cursor, "known varint").unwrap(),
            *expected
        );
        assert!(cursor.is_empty());
    }

    let lowz_vectors: &[(&[u8], u64)] = &[
        (&[0x07], 0x1000),
        (&[0x4b], 0x12_000),
        (&[0x8f, 0x09], 0x123_000),
    ];
    for (encoded, expected) in lowz_vectors {
        let mut cursor = CephCursor::new(encoded);
        assert_eq!(
            decode_varint_lowz_u64(&mut cursor, "known lowz").unwrap(),
            *expected
        );
        assert!(cursor.is_empty());
    }
}

#[test]
fn lba_decoder_covers_all_selector_families_and_continuations() {
    let vectors: &[(&[u8], u64)] = &[
        (&[0x02, 0x00, 0x00, 0x00], 0x1000),
        (&[0x09, 0x00, 0x00, 0x00], 0x20_000),
        (&[0x0b, 0x00, 0x00, 0x00], 0x100_000),
        (&[0x1f, 0x09, 0x00, 0x00], 0x123),
        (&[0x03, 0x00, 0x00, 0x80, 0x04], 1u64 << 50),
        (&[0x1f, 0x09, 0x00, 0x80, 0x80, 0x20], (1u64 << 40) | 0x123),
    ];
    for (encoded, expected) in vectors {
        let mut cursor = CephCursor::new(encoded);
        assert_eq!(decode_lba_u64(&mut cursor, "known LBA").unwrap(), *expected);
        assert!(cursor.is_empty());
    }
}
