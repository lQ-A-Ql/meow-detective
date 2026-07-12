use super::*;
use std::io::Cursor;

// --- find_magic ---

#[test]
fn find_magic_at_start() {
    assert_eq!(find_magic(b"\xFF\xD8\xFF\xE0", b"\xFF\xD8\xFF"), Some(0));
}

#[test]
fn find_magic_at_middle() {
    let data = b"garbage\x50\x4B\x03\x04more";
    assert_eq!(find_magic(data, b"\x50\x4B\x03\x04"), Some(7));
}

#[test]
fn find_magic_not_found() {
    assert_eq!(find_magic(b"no match here", b"\xFF\xD8\xFF"), None);
}

#[test]
fn find_magic_empty_needle() {
    assert_eq!(find_magic(b"anything", b""), None);
}

#[test]
fn find_magic_needle_longer_than_haystack() {
    assert_eq!(find_magic(b"short", b"this is way too long"), None);
}

// --- JPEG carving ---

#[test]
fn carve_jpeg_with_header_and_footer() {
    let header = b"\xFF\xD8\xFF\xE0";
    let body = vec![0xAAu8; 1000];
    let footer = b"\xFF\xD9";
    let mut data = Vec::new();
    data.extend_from_slice(header);
    data.extend_from_slice(&body);
    data.extend_from_slice(footer);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");

    assert_eq!(carved.len(), 1, "expected 1 carved JPEG");
    assert_eq!(carved[0].file_type, "jpeg");
    assert_eq!(carved[0].offset, 0);
    assert_eq!(carved[0].size, data.len() as u64);
    assert!((carved[0].confidence - 1.0).abs() < 0.001);
    assert_eq!(&carved[0].data, &data);
}

#[test]
fn carve_jpeg_without_footer_truncates() {
    let header = b"\xFF\xD8\xFF\xE0";
    let body = vec![0xBBu8; 500];
    let mut data = Vec::new();
    data.extend_from_slice(header);
    data.extend_from_slice(&body);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");

    assert_eq!(carved.len(), 1);
    assert_eq!(carved[0].file_type, "jpeg");
    assert!(carved[0].confidence < 0.5, "no footer = low confidence");
}

// --- ZIP carving ---

#[test]
fn carve_zip_with_eocd() {
    // Build a synthetic ZIP: local file header + central directory + EOCD
    let local_header = b"\x50\x4B\x03\x04\x14\x00\x00\x00\x00\x00";
    let file_data = vec![0xCCu8; 200];
    let central_dir = b"\x50\x4B\x01\x02\x14\x00\x00\x00\x00\x00";
    let eocd_magic = b"\x50\x4B\x05\x06";
    let eocd_record = vec![0u8; 18]; // 22-byte EOCD record

    let mut data = Vec::new();
    data.extend_from_slice(local_header);
    data.extend_from_slice(&file_data);
    data.extend_from_slice(central_dir);
    data.extend_from_slice(eocd_magic);
    data.extend_from_slice(&eocd_record);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");

    assert_eq!(carved.len(), 1);
    assert_eq!(carved[0].file_type, "zip");
    assert_eq!(carved[0].offset, 0);
    assert!((carved[0].confidence - 1.0).abs() < 0.001);
    // Verify it captured through the EOCD
    let last_four = &carved[0].data[carved[0].data.len() - 4..];
    assert_eq!(last_four, eocd_magic);
}

#[test]
fn carve_zip_without_eocd_limited_confidence() {
    let local_header = b"\x50\x4B\x03\x04\x14\x00\x00\x00\x00\x00";
    let file_data = vec![0xDDu8; 300];
    let mut data = Vec::new();
    data.extend_from_slice(local_header);
    data.extend_from_slice(&file_data);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");

    assert_eq!(carved.len(), 1);
    assert_eq!(carved[0].file_type, "zip");
    assert!(carved[0].confidence < 0.5);
}

// --- PDF carving ---

#[test]
fn carve_pdf_with_eof() {
    let header = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n";
    let body = vec![0xEEu8; 800];
    let footer = b"\n%%EOF\n";
    let mut data = Vec::new();
    data.extend_from_slice(header);
    data.extend_from_slice(&body);
    data.extend_from_slice(footer);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");

    assert_eq!(carved.len(), 1);
    assert_eq!(carved[0].file_type, "pdf");
    assert!((carved[0].confidence - 1.0).abs() < 0.001);
}

#[test]
fn carve_pdf_pdf_eof_only_header_truncates() {
    let header = b"%PDF-1.4\n";
    let body = vec![0x11u8; 1200];
    let mut data = Vec::new();
    data.extend_from_slice(header);
    data.extend_from_slice(&body);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");

    assert_eq!(carved.len(), 1);
    assert_eq!(carved[0].file_type, "pdf");
    assert!(carved[0].confidence < 0.5);
}

// --- Multi-type carving ---

#[test]
fn carve_multiple_formats_in_one_stream() {
    // JPEG first, then ZIP later in the stream
    let jpeg_header = b"\xFF\xD8\xFF\xE0";
    let jpeg_body = vec![0xAAu8; 50];
    let jpeg_footer = b"\xFF\xD9";

    let gap = vec![0u8; 200];

    let zip_header = b"\x50\x4B\x03\x04\x14\x00\x00\x00\x00\x00";
    let zip_body = vec![0xBBu8; 100];
    let eocd =
        b"\x50\x4B\x05\x06\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

    let mut data = Vec::new();
    data.extend_from_slice(jpeg_header);
    data.extend_from_slice(&jpeg_body);
    data.extend_from_slice(jpeg_footer);
    data.extend_from_slice(&gap);
    data.extend_from_slice(zip_header);
    data.extend_from_slice(&zip_body);
    data.extend_from_slice(eocd);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");

    assert!(
        carved.len() >= 2,
        "expected at least 2 files, got {}",
        carved.len()
    );
    let types: Vec<&str> = carved.iter().map(|c| c.file_type.as_str()).collect();
    assert!(types.contains(&"jpeg"), "missing jpeg");
    assert!(types.contains(&"zip"), "missing zip");
}

#[test]
fn empty_stream_returns_empty() {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");
    assert!(carved.is_empty());
}

#[test]
fn stream_with_no_known_headers_returns_empty() {
    let data = vec![0x00u8; 1024];
    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");
    assert!(carved.is_empty());
}

#[test]
fn carve_jpeg_multiple_in_stream() {
    let jpeg1 = {
        let header = b"\xFF\xD8\xFF\xE0";
        let body = vec![0x11u8; 60];
        let footer = b"\xFF\xD9";
        let mut d = Vec::new();
        d.extend_from_slice(header);
        d.extend_from_slice(&body);
        d.extend_from_slice(footer);
        d
    };
    let jpeg2 = {
        let header = b"\xFF\xD8\xFF\xE1";
        let body = vec![0x22u8; 40];
        let footer = b"\xFF\xD9";
        let mut d = Vec::new();
        d.extend_from_slice(header);
        d.extend_from_slice(&body);
        d.extend_from_slice(footer);
        d
    };

    let mut data = Vec::new();
    data.extend_from_slice(&[0u8; 10]); // garbage prefix
    data.extend_from_slice(&jpeg1);
    data.extend_from_slice(&[0xFFu8; 30]); // gap
    data.extend_from_slice(&jpeg2);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");

    let jpeg_count = carved.iter().filter(|c| c.file_type == "jpeg").count();
    assert_eq!(jpeg_count, 2, "expected 2 JPEGs, got {}", jpeg_count);

    // First JPEG should have non-zero offset due to garbage prefix.
    assert!(
        carved[0].offset >= 10,
        "first JPEG offset should account for prefix"
    );
}

#[test]
fn pdf_specific_version_detected() {
    // PDF 1.7
    let header = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n";
    let body = vec![0x33u8; 200];
    let footer = b"%%EOF";
    let mut data = Vec::new();
    data.extend_from_slice(header);
    data.extend_from_slice(&body);
    data.extend_from_slice(footer);

    let mut cursor = Cursor::new(&data);
    let carved = carve_from_unallocated(&mut cursor, "raw").expect("carve");
    assert_eq!(carved.len(), 1);
    assert_eq!(carved[0].file_type, "pdf");
    assert!((carved[0].confidence - 1.0).abs() < 0.001);
}
