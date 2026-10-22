use crate::foxmail::parse_box;
use crate::foxmail::{
    decode_account_password, is_index, is_list_container, parse_account_info,
    parse_account_password, parse_index, FoxmailAccountVersion, FoxmailPasswordError,
};

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

#[test]
fn parses_foxmail7_index_header_fields_and_ole_time() {
    let mut data = vec![0u8; 0x400];
    data[..8].copy_from_slice(b"FXIS\x02\0\x01\0");
    let record = &mut data[0x200..0x400];
    record[..4].copy_from_slice(&1u32.to_le_bytes());
    record[0x08..0x10].copy_from_slice(&46282.5f64.to_le_bytes());
    record[0x10..0x18].copy_from_slice(&46282.25f64.to_le_bytes());
    let fields = [
        "Sender",
        "sender@example.com",
        "Recipient",
        "recipient@example.com",
        "Subject",
    ];
    for (slot, field) in fields.iter().enumerate() {
        record[0x2d + slot] = u8::try_from(field.len()).unwrap();
    }
    let mut offset = 0x33;
    for field in fields {
        record[offset..offset + field.len()].copy_from_slice(field.as_bytes());
        offset += field.len();
    }

    let parsed = parse_index(&data);
    assert_eq!(parsed.invalid_record_count, 0);
    assert_eq!(parsed.records.len(), 1);
    let message = &parsed.records[0];
    assert_eq!(message.id, 1);
    assert_eq!(message.from_email, "sender@example.com");
    assert_eq!(message.to_email, "recipient@example.com");
    assert_eq!(message.subject, "Subject");
    assert_eq!(
        message.primary_time.unwrap().to_rfc3339(),
        "2026-09-17T12:00:00+00:00"
    );
}

#[test]
fn recognizes_foxmail7_index_and_list_containers() {
    let mut index = vec![0u8; 0x200];
    index[..8].copy_from_slice(b"FXIS\x02\0\x01\0");
    assert!(is_index(&index));
    assert!(is_list_container(b"LSTG\x01\0\x01\0"));
    assert!(!is_index(b"LSTG\x01\0\x01\0"));
}

#[test]
fn decodes_foxmail7_account_password_algorithm() {
    let password = decode_account_password(
        FoxmailAccountVersion::V7,
        b"C67C97BE146CA2290F28C67DE747F96CE1",
    )
    .unwrap();
    assert_eq!(password, "JUgd5cjHGXvSzMN3");
}

#[test]
fn parses_latest_password_field_from_account_rec0() {
    let mut data = vec![0x52];
    data.extend_from_slice(b"\0Password\0\0\0\0\x04\0\0\0dead");
    data.extend_from_slice(b"\0Password\0\0\0\0\x22\0\0\0C67C97BE146CA2290F28C67DE747F96CE1");
    let parsed = parse_account_password(&data).unwrap();
    assert_eq!(parsed.version, FoxmailAccountVersion::V7);
    assert_eq!(parsed.password, "JUgd5cjHGXvSzMN3");
}

#[test]
fn rejects_unknown_account_version_and_malformed_hex() {
    assert_eq!(
        parse_account_password(b"\x01\0Password").unwrap_err(),
        FoxmailPasswordError::UnsupportedVersion(1)
    );
    assert_eq!(
        decode_account_password(FoxmailAccountVersion::V7, b"zz").unwrap_err(),
        FoxmailPasswordError::InvalidHex
    );
}

#[test]
fn parses_non_secret_foxmail_account_metadata() {
    fn field(key: &str, value: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(key.len() as u32).to_le_bytes());
        bytes.extend_from_slice(key.as_bytes());
        bytes.extend_from_slice(&0x100u32.to_le_bytes());
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(value);
        bytes
    }

    let mut data = vec![0x52];
    data.extend(field("Account", b"person@example.com"));
    data.extend(field("Email", b"person@example.com"));
    data.extend(field("IncomingServer", b"imap.example.com"));
    data.extend(field("OutgoingServer", b"smtp.example.com"));
    let parsed = parse_account_info(&data).unwrap();
    assert_eq!(parsed.account.as_deref(), Some("person@example.com"));
    assert_eq!(parsed.email.as_deref(), Some("person@example.com"));
    assert_eq!(parsed.incoming_server.as_deref(), Some("imap.example.com"));
    assert_eq!(parsed.outgoing_server.as_deref(), Some("smtp.example.com"));
}
