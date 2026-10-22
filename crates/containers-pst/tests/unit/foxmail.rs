use crate::foxmail::parse_box;

const RECORD_SEPARATOR: [u8; 16] = [
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, b'S', b'\r',
    b'\n',
];

fn sample_message(subject: &str) -> Vec<u8> {
    format!(
        "From: sender@example.com\r\nSubject: {subject}\r\nDate: Tue, 21 Oct 2026 10:00:00 +0000\r\n\r\nBody"
    )
    .into_bytes()
}

#[test]
fn splits_bounded_records_and_trailing_record() {
    let first = sample_message("first");
    let second = sample_message("second");
    let mut data = first.clone();
    data.extend_from_slice(&RECORD_SEPARATOR);
    data.extend_from_slice(&second);

    let parsed = parse_box(&data);
    assert_eq!(parsed.separator_count, 1);
    assert_eq!(parsed.records, vec![first, second]);
    assert_eq!(parsed.ignored_record_count, 0);
}

#[test]
fn ignores_binary_segments_without_rfc822_headers() {
    let mut data = b"binary metadata".to_vec();
    data.extend_from_slice(&RECORD_SEPARATOR);
    data.extend_from_slice(b"still binary");

    let parsed = parse_box(&data);
    assert_eq!(parsed.records, Vec::<Vec<u8>>::new());
    assert_eq!(parsed.ignored_record_count, 1);
}

#[test]
fn ordinary_box_without_separator_is_not_detected() {
    let parsed = parse_box(b"From: sender@example.com\r\nSubject: test\r\n\r\nbody");
    assert_eq!(parsed.separator_count, 0);
    assert!(parsed.records.is_empty());
}
