use super::marvin64;

#[test]
fn fixed_seed_hash_matches_independent_registry_reference_vectors() {
    for (size, expected) in [
        (0, 0xb39efca403966e08u64),
        (4, 0x2400585e2d116fbf),
        (32, 0xda91a06ac5359223),
        (472, 0xb16122d0c0639acc),
    ] {
        assert_eq!(marvin64(&vec![0; size]), expected);
    }
}
