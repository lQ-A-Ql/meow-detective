use super::*;

/// Create a valid test backup header with the correct magic at the start.
fn make_test_header(version: u32, flags: u32, salt_length: u16) -> Vec<u8> {
    let mut buf = Vec::new();
    // Magic: "ANDROID BACKUP" (14 bytes, including trailing space)
    // A=65 N=78 D=68 R=82 O=79 I=73 D=68 space=32 B=66 A=65 C=67 K=75 U=85 P=80
    buf.extend_from_slice(&[65, 78, 68, 82, 79, 73, 68, 32, 66, 65, 67, 75, 85, 80]);
    // version: 4 bytes LE
    buf.extend_from_slice(&version.to_le_bytes());
    // flags: 4 bytes LE
    buf.extend_from_slice(&flags.to_le_bytes());
    // salt length: 2 bytes LE
    buf.extend_from_slice(&salt_length.to_le_bytes());
    buf
}

#[test]
fn parse_empty_data_returns_error() {
    let result = parse_backup_header(&[]);
    assert!(result.is_err());
}

#[test]
fn parse_non_backup_data_returns_none() {
    let data = b"this is just some random binary data not an adb backup";
    let result = parse_backup_header(data).expect("should not error");
    assert!(result.is_none());
}

#[test]
fn parse_valid_header_uncompressed() {
    let data = make_test_header(3, 0, 0);
    let result = parse_backup_header(&data).expect("should parse");
    assert!(result.is_some());
    let header = result.unwrap();
    assert_eq!(header.version, 3);
    assert!(!header.is_compressed);
    assert!(!header.is_encrypted);
    assert_eq!(header.salt_length, 0);
}

#[test]
fn parse_valid_header_compressed() {
    let data = make_test_header(4, 1, 0);
    let result = parse_backup_header(&data).expect("should parse");
    assert!(result.is_some());
    let header = result.unwrap();
    assert_eq!(header.version, 4);
    assert!(header.is_compressed);
    assert!(!header.is_encrypted);
}

#[test]
fn parse_valid_header_encrypted() {
    let data = make_test_header(5, 0, 32);
    let result = parse_backup_header(&data).expect("should parse");
    assert!(result.is_some());
    let header = result.unwrap();
    assert_eq!(header.version, 5);
    assert!(!header.is_compressed);
    assert!(header.is_encrypted);
    assert_eq!(header.salt_length, 32);
}

#[test]
fn parse_truncated_header_with_magic_returns_error() {
    // Only the magic bytes, no room for version/flags/salt
    let mut data = Vec::new();
    data.extend_from_slice(&[65, 78, 68, 82, 79, 73, 68, 32, 66, 65, 67, 75, 85, 80]);
    let result = parse_backup_header(&data);
    assert!(result.is_err());
}
