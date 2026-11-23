use super::{analyze, viewer, ImageAnalysisStatus, ImageAnalyzeOptions};
use image_inspection::ico_codec::{IconDir, IconDirEntry, IconImage, ResourceType};
use std::io::Cursor;

pub(super) fn encoded(dimensions: &[(u32, u32)]) -> Vec<u8> {
    let mut directory = IconDir::new(ResourceType::Icon);
    for (index, &(width, height)) in dimensions.iter().enumerate() {
        let image =
            IconImage::from_rgba_data(width, height, vec![255; (width * height * 4) as usize]);
        let entry = if index % 2 == 0 {
            IconDirEntry::encode_as_png(&image).unwrap()
        } else {
            IconDirEntry::encode_as_bmp(&image).unwrap()
        };
        directory.add_entry(entry);
    }
    let mut bytes = Vec::new();
    directory.write(&mut bytes).unwrap();
    bytes
}

#[test]
fn native_ico_directory_matches_png_and_bmp_payloads_and_source_extents() {
    let bytes = encoded(&[(2, 3), (16, 12), (256, 256)]);
    let metadata = IconDir::read_metadata(Cursor::new(&bytes), 1024).unwrap();
    assert_eq!(metadata.entries.len(), 3);
    let decoded = IconDir::read(Cursor::new(&bytes)).unwrap();
    for (entry, native) in metadata.entries.iter().zip(decoded.entries()) {
        assert_eq!(
            (entry.width, entry.height),
            (native.width(), native.height())
        );
        assert_eq!(entry.length as usize, native.data().len());
        assert_eq!(
            &bytes[entry.offset as usize..][..entry.length as usize],
            native.data()
        );
    }
    let report = viewer(bytes.clone());
    assert_eq!(report.status, ImageAnalysisStatus::Absent);
    assert_eq!(report.dimensions, Some((256, 256)));
    let structure = report.structure.unwrap();
    assert_eq!(structure.declared_end, Some(bytes.len() as u64));
    for (range, entry) in structure.segments.iter().zip(metadata.entries) {
        assert_eq!(
            (range.offset, range.length),
            (u64::from(entry.offset), u64::from(entry.length))
        );
    }
}

#[test]
fn native_ico_directory_limit_precedes_image_loading_and_table_allocation() {
    let bytes = encoded(&vec![(1, 1); 1025]);
    let error = IconDir::read_metadata(Cursor::new(&bytes[..6]), 1024).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::FileTooLarge);
    let report = viewer(bytes);
    assert_eq!(report.status, ImageAnalysisStatus::Oversized);
    assert_eq!(report.structure.unwrap().declared_end, None);
}

#[test]
fn native_ico_dangling_payload_span_does_not_allocate_declared_image_bytes() {
    let mut bytes = encoded(&[(2, 3)]);
    bytes[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
    let report = viewer(bytes.clone());
    assert!(report.bytes_read <= bytes.len() as u64 + 16);
    let structure = report.structure.unwrap();
    assert!(structure.truncated);
    assert_eq!(structure.declared_end, None);
    assert!(IconDir::read(Cursor::new(bytes)).is_err());
}

#[test]
fn native_ico_reserved_fields_and_overlapping_directory_spans_are_rejected() {
    let bytes = encoded(&[(2, 3)]);
    for reserved in [true, false] {
        let mut invalid = bytes.clone();
        if reserved {
            invalid[0] = 1;
        } else {
            invalid[18..22].copy_from_slice(&0u32.to_le_bytes());
        }
        let report = viewer(invalid);
        if reserved {
            assert_eq!(report.status, ImageAnalysisStatus::Unsupported);
        } else {
            assert_eq!(report.status, ImageAnalysisStatus::Corrupt);
            assert_eq!(report.structure.unwrap().declared_end, None);
        }
    }
}

#[test]
fn native_ico_trailing_hash_and_read_budget_preserve_the_container_boundary() {
    let mut bytes = encoded(&[(2, 3)]);
    let expected_end = bytes.len() as u64;
    bytes.extend_from_slice(b"tail");
    let bounded = analyze(
        bytes.clone(),
        ImageAnalyzeOptions {
            random_read_limit: 32,
            ..ImageAnalyzeOptions::viewer()
        },
    );
    assert_eq!(bounded.status, ImageAnalysisStatus::Oversized);
    assert_eq!(bounded.structure.unwrap().declared_end, None);
    let report = analyze(bytes, ImageAnalyzeOptions::analysis());
    let structure = report.structure.unwrap();
    assert_eq!(structure.declared_end, Some(expected_end));
    assert_eq!(structure.trailing_bytes, Some(4));
    assert_eq!(structure.hashes.trailing_hashed_bytes, Some(4));
}
