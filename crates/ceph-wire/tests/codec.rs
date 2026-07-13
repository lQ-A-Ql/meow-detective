use std::collections::BTreeMap;

use ceph_wire::{
    codec::{decode_string, decode_string_map, CephDecode, CephEncode},
    CephCursor, CephStructEnvelope, CephUtime, CephWireError,
};
use uuid::Uuid;

#[test]
fn cursor_rejects_reads_past_the_bound() {
    let mut cursor = CephCursor::new(&[1, 2, 3]);
    assert_eq!(cursor.read_exact(2).unwrap(), &[1, 2]);
    assert_eq!(
        cursor.read_exact(2).unwrap_err(),
        CephWireError::UnexpectedEof {
            offset: 2,
            needed: 2,
            remaining: 1,
        }
    );
}

#[test]
fn envelope_checks_compat_and_bounds_payload() {
    let bytes = [3, 3, 2, 0, 0, 0, 9, 8];
    let mut cursor = CephCursor::new(&bytes);
    assert!(matches!(
        CephStructEnvelope::decode_payload(&mut cursor, 2),
        Err(CephWireError::IncompatibleStructVersion { .. })
    ));

    let bytes = [2, 1, 3, 0, 0, 0, 9, 8];
    let mut cursor = CephCursor::new(&bytes);
    assert!(matches!(
        CephStructEnvelope::decode_payload(&mut cursor, 2),
        Err(CephWireError::UnexpectedEof { .. })
    ));
}

#[test]
fn primitive_uuid_utime_string_and_map_round_trip() {
    let uuid = Uuid::parse_str("12345678-1234-5678-90ab-cdef01234567").unwrap();
    let time = CephUtime {
        seconds: 1_700_000_000,
        nanoseconds: 987_654_321,
    };
    let mut map = BTreeMap::new();
    map.insert("multi".to_string(), "yes".to_string());
    map.insert("用途".to_string(), "只读".to_string());

    let mut bytes = Vec::new();
    uuid.encode(&mut bytes);
    time.encode(&mut bytes);
    "BlueStore 数据".to_string().encode(&mut bytes);
    map.encode(&mut bytes);

    let mut cursor = CephCursor::new(&bytes);
    assert_eq!(Uuid::decode(&mut cursor).unwrap(), uuid);
    assert_eq!(CephUtime::decode(&mut cursor).unwrap(), time);
    assert_eq!(String::decode(&mut cursor).unwrap(), "BlueStore 数据");
    assert_eq!(
        BTreeMap::<String, String>::decode(&mut cursor).unwrap(),
        map
    );
    assert!(cursor.is_empty());
}

#[test]
fn string_and_map_limits_are_enforced_before_allocation() {
    let encoded_length = 100u32.to_le_bytes();
    let mut cursor = CephCursor::new(&encoded_length);
    assert!(matches!(
        decode_string(&mut cursor, 10, "test string"),
        Err(CephWireError::LengthLimit { .. })
    ));

    let mut cursor = CephCursor::new(&encoded_length);
    assert!(matches!(
        decode_string_map(&mut cursor, 10, 10),
        Err(CephWireError::LengthLimit { .. })
    ));
}

#[test]
fn invalid_utf8_is_typed() {
    let bytes = [1, 0, 0, 0, 0xff];
    let mut cursor = CephCursor::new(&bytes);
    assert!(matches!(
        decode_string(&mut cursor, 10, "bad string"),
        Err(CephWireError::InvalidUtf8 { .. })
    ));
}
