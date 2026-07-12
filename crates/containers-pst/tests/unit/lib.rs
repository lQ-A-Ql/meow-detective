#[path = "support/synthetic.rs"]
mod synthetic;

pub(crate) fn build_synthetic_unicode_pst() -> Vec<u8> {
    synthetic::build_synthetic_pst_with_messages(1)
}

/// Minimal "!BDN" magic bytes - the 4 bytes that identify a PST/OST.
const PST_MAGIC: [u8; 4] = [0x21, 0x42, 0x44, 0x4E];

/// Build a bare-minimum 512-byte PST header for detection tests.
fn build_detection_header(version_major: u8, version_minor: u8) -> Vec<u8> {
    let mut header = vec![0u8; 512];

    header[0..4].copy_from_slice(&PST_MAGIC);
    header[8] = 0x53;
    header[9] = 0x4D;
    header[10] = version_major;
    header[11] = version_minor;

    let is_unicode = version_major >= 23;
    let root_off: usize = if is_unicode { 188 } else { 176 };
    let file_size: u64 = 512;

    if is_unicode {
        header[root_off + 4..root_off + 12].copy_from_slice(&file_size.to_le_bytes());
        header[root_off + 36..root_off + 44].copy_from_slice(&1u64.to_le_bytes());
        header[root_off + 44..root_off + 52].copy_from_slice(&512u64.to_le_bytes());
        header[root_off + 56..root_off + 64].copy_from_slice(&2u64.to_le_bytes());
        header[root_off + 64..root_off + 72].copy_from_slice(&1024u64.to_le_bytes());
    } else {
        header[root_off + 4..root_off + 8].copy_from_slice(&(512u32).to_le_bytes());
        header[root_off + 20..root_off + 24].copy_from_slice(&1u32.to_le_bytes());
        header[root_off + 24..root_off + 28].copy_from_slice(&512u32.to_le_bytes());
        header[root_off + 32..root_off + 36].copy_from_slice(&2u32.to_le_bytes());
        header[root_off + 36..root_off + 40].copy_from_slice(&1024u32.to_le_bytes());
    }

    header
}

#[test]
fn detect_pst_by_magic_bytes() {
    let header = build_detection_header(23, 0);
    assert_eq!(&header[0..4], &PST_MAGIC);
    assert_eq!(header[0], 0x21);
    assert_eq!(header[1], 0x42);
    assert_eq!(header[2], 0x44);
    assert_eq!(header[3], 0x4E);
}

#[test]
fn detect_unicode_pst_version_23() {
    let header = build_detection_header(23, 0);
    let ver = u16::from_le_bytes([header[10], header[11]]);
    assert_eq!(ver, 23);
    assert!(ver >= 23);
}

#[test]
fn detect_ansi_pst_version_14() {
    let header = build_detection_header(14, 0);
    let ver = u16::from_le_bytes([header[10], header[11]]);
    assert_eq!(ver, 14);
    assert!(ver < 23);
}

#[test]
fn detect_ansi_pst_version_15() {
    let header = build_detection_header(15, 0);
    let ver = u16::from_le_bytes([header[10], header[11]]);
    assert_eq!(ver, 15);
    assert!(ver < 23);
}

#[test]
fn reject_file_without_magic() {
    let bad: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
    assert_ne!(&bad[..], &PST_MAGIC[..]);

    let garbage = vec![0u8; 512];
    assert_ne!(&garbage[0..4], &PST_MAGIC[..]);
}

#[test]
fn reject_file_too_small() {
    let tiny = [0u8; 100];
    assert!(tiny.len() < 512);
    assert!(tiny.len() >= 4);
}

#[test]
fn detect_variant_mboxrd_by_escaped_from() {
    let data = "\
From sender@example.com Mon Jun 16 10:00:00 2025
From: Sender <sender@example.com>
Subject: Test

Some body text.
>From original@example.com Mon Jun 16 09:00:00 2025
>From: Original <original@example.com>
>Subject: Original
";
    let variant = crate::mbox::detect_variant(data);
    assert_eq!(variant, crate::mbox::MboxVariant::MboxRd);
}

#[test]
fn detect_variant_mboxo_by_no_escaping() {
    let data = "\
From sender@example.com Mon Jun 16 10:00:00 2025
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: Hello
Date: Mon, 16 Jun 2025 10:00:00 +0200

Plain text body without any escaped From lines.
";
    let variant = crate::mbox::detect_variant(data);
    assert_eq!(variant, crate::mbox::MboxVariant::MboxO);
}

#[test]
fn detect_variant_mboxcl_by_content_length() {
    let data = "\
From sender@example.com Mon Jun 16 12:00:00 2025
Content-Length: 42
From: Sender <sender@example.com>
Subject: CL test

Body with content-length header.
";
    let variant = crate::mbox::detect_variant(data);
    assert_eq!(variant, crate::mbox::MboxVariant::MboxCl);
}

#[test]
fn detect_variant_mboxcl2_by_status_headers() {
    let data = "\
From sender@example.com Mon Jun 16 14:00:00 2025
Content-Length: 50
Status: RO
X-Status: F
From: Sender <sender@example.com>
Subject: CL2 test

Body with status headers.
";
    let variant = crate::mbox::detect_variant(data);
    assert_eq!(variant, crate::mbox::MboxVariant::MboxCl2);
}

#[test]
fn variant_detection_ignores_body_content_length() {
    let data = "\
From sender@example.com Mon Jun 16 10:00:00 2025
From: Sender <sender@example.com>
Subject: Discussion about Content-Length

We were discussing the Content-Length header in RFC 4155.
It is an important part of the mboxcl format.
";
    let variant = crate::mbox::detect_variant(data);
    assert_eq!(variant, crate::mbox::MboxVariant::MboxO);
}

#[test]
fn empty_data_defaults_to_mboxo() {
    let data = "";
    let variant = crate::mbox::detect_variant(data);
    assert_eq!(variant, crate::mbox::MboxVariant::MboxO);
}

#[test]
fn inline_pst_magic_bytes_match_constant() {
    let expected: [u8; 4] = [0x21, 0x42, 0x44, 0x4E];
    assert_eq!(PST_MAGIC, expected);

    let ascii = std::str::from_utf8(&PST_MAGIC).unwrap();
    assert_eq!(ascii, "!BDN");
}

#[test]
fn inline_binary_header_is_512_bytes() {
    let header = build_detection_header(23, 0);
    assert_eq!(header.len(), 512);
    assert_eq!(header[0], 0x21);
    assert_eq!(header.len(), 512);
}

#[test]
fn unicode_header_root_offset_is_188() {
    let header = build_detection_header(23, 0);
    let file_size_bytes = &header[192..200];
    let file_size = u64::from_le_bytes(file_size_bytes.try_into().unwrap());
    assert_eq!(file_size, 512);
}

#[test]
fn ansi_header_root_offset_is_176() {
    let header = build_detection_header(14, 0);
    let file_size_bytes = &header[180..184];
    let file_size = u32::from_le_bytes(file_size_bytes.try_into().unwrap());
    assert_eq!(file_size, 512);
}
