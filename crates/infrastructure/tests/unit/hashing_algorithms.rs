use super::*;

#[test]
fn standard_algorithms_match_known_vectors() {
    assert_eq!(
        digest_bytes(b"abc", HashAlgorithm::Md5),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(
        digest_bytes(b"abc", HashAlgorithm::Sha1),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        digest_bytes(b"abc", HashAlgorithm::Sha256),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        digest_bytes(b"abc", HashAlgorithm::Sm3),
        "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0"
    );
}

#[test]
fn sm3_matches_standard_vectors_across_block_boundaries() {
    let vectors = [
        (
            0,
            "1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b",
        ),
        (
            55,
            "288337eef51eec62e7544d7270424c8dbe656254c99852870a73b2453a6a7fb1",
        ),
        (
            56,
            "ba00ebedaab54065a5fd4f9f56326016203166bcee3eed44ea868d59d67aa3c8",
        ),
        (
            64,
            "616ec433c359e7c2b19f360e2b8f2a1b6e9ed76b8dc1a7d207b31a5341c611e9",
        ),
        (
            1_000_000,
            "c8aaf89429554029e231941a2acc0ad61ff2a5acd8fadd25847a3a732b3b02c3",
        ),
    ];
    for (length, expected) in vectors {
        assert_eq!(
            digest_bytes(&vec![b'a'; length], HashAlgorithm::Sm3),
            expected
        );
    }
}

#[test]
fn streaming_digest_can_be_cancelled() {
    let mut reader = std::io::Cursor::new(vec![0u8; 1024]);
    let result = digest_reader_with_cancel(&mut reader, HashAlgorithm::Sha256, || true, |_| {});
    assert_eq!(result.unwrap(), None);
}
