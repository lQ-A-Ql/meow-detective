use super::*;

#[test]
fn bounded_codec_matches_native_provider_and_rejects_oversized_output() {
    let input = [
        0x03, 0xb0, 0x02, b'A', 0xfc, 0x0f, 0x03, 0xb0, 0x02, b'A', 0x6c, 0x07,
    ];
    let portable = decode_lznt1_portable(&input, 8192).unwrap();
    assert_eq!(portable, vec![b'A'; 6000]);
    assert_eq!(decode_lznt1(&input, 8192).unwrap(), portable);
    assert!(decode_lznt1_portable(&input, 4096).is_err());
    assert!(decode_lznt1_portable(&[0xff, 0xff], 8192).is_err());
}
