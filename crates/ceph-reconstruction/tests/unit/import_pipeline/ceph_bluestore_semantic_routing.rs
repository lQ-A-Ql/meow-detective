use ceph_wire::BlueStoreKeySpace;

use super::route_bluestore_key;
use crate::import_pipeline::ceph_rocksdb_sharding::parse_rocksdb_sharding_definition;

#[test]
fn routes_supported_default_column_family_prefixes_without_copying() {
    let sharding = parse_rocksdb_sharding_definition("O").expect("parse sharding");
    for (prefix, expected) in [
        (b'S', BlueStoreKeySpace::Super),
        (b'C', BlueStoreKeySpace::Collection),
        (b'O', BlueStoreKeySpace::Object),
        (b'X', BlueStoreKeySpace::SharedBlob),
    ] {
        let key = [prefix, 0, b'k', b'e', b'y'];
        let routed = route_bluestore_key(&sharding, "default", &key)
            .expect("route default key")
            .expect("supported key");
        assert_eq!(routed.key_space, expected);
        assert_eq!(routed.logical_key, b"key");
        assert_eq!(routed.logical_key.as_ptr(), key[2..].as_ptr());
    }
}

#[test]
fn routes_prefix_stripped_dedicated_column_family_keys() {
    let sharding = parse_rocksdb_sharding_definition("O(3)").expect("parse sharding");
    let key = b"encoded-object-key";
    let routed = route_bluestore_key(&sharding, "O-2", key)
        .expect("route dedicated key")
        .expect("supported key");

    assert_eq!(routed.key_space, BlueStoreKeySpace::Object);
    assert_eq!(routed.logical_key, key);
    assert_eq!(routed.logical_key.as_ptr(), key.as_ptr());
}

#[test]
fn ignores_known_but_not_yet_semantically_supported_key_spaces() {
    let sharding = parse_rocksdb_sharding_definition("m(3) p(3) L P").expect("parse sharding");
    assert!(route_bluestore_key(&sharding, "default", b"T\0stat")
        .expect("route stat key")
        .is_none());
    assert!(route_bluestore_key(&sharding, "m-0", b"omap")
        .expect("route omap key")
        .is_none());
}

#[test]
fn rejects_malformed_or_unvalidated_physical_routes() {
    let sharding = parse_rocksdb_sharding_definition("O").expect("parse sharding");
    assert!(route_bluestore_key(&sharding, "default", b"Oobject").is_err());
    assert!(route_bluestore_key(&sharding, "missing", b"object").is_err());
}
