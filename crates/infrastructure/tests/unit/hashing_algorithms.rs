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
fn streaming_digest_can_be_cancelled() {
    let mut reader = std::io::Cursor::new(vec![0u8; 1024]);
    let result = digest_reader_with_cancel(&mut reader, HashAlgorithm::Sha256, || true, |_| {});
    assert_eq!(result.unwrap(), None);
}
