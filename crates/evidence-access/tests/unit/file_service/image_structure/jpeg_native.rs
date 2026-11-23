use super::{analyze, fixture, viewer, ImageAnalysisStatus, ImageAnalyzeOptions};
use image_inspection::jpeg_codec::Decoder;
use img_parts::{
    jpeg::{Jpeg, JpegSegment},
    Bytes, ImageEXIF,
};
use std::io::Cursor;

pub(super) const BASELINE: &[u8] = include_bytes!("../../../fixtures/image-structure/baseline.jpg");
const PROGRESSIVE: &[u8] = include_bytes!("../../../fixtures/image-structure/progressive.jpg");

pub(super) fn with_exif(exif: &[u8], progressive: bool) -> Vec<u8> {
    assert!(exif.len() <= 65527);
    let bytes = if progressive { PROGRESSIVE } else { BASELINE };
    let mut jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(bytes)).unwrap();
    jpeg.set_exif(Some(Bytes::copy_from_slice(exif)));
    assert_eq!(jpeg.exif().as_deref(), Some(exif));
    jpeg.encoder().bytes().to_vec()
}

#[test]
fn native_jpeg_scanner_covers_baseline_and_all_progressive_scans() {
    for progressive in [false, true] {
        let bytes = with_exif(&fixture::tiff(true), progressive);
        let mut codec = Decoder::new(Cursor::new(&bytes));
        assert_eq!(codec.decode().unwrap().len(), 18);
        let info = codec.info().unwrap();
        assert_eq!((info.width, info.height), (2, 3));
        let report = viewer(bytes.clone());
        assert_eq!(report.status, ImageAnalysisStatus::Present);
        assert_eq!(report.dimensions, Some((2, 3)));
        let block = &report.exif_blocks[0];
        assert_eq!(
            &bytes[block.offset as usize..][..block.length as usize],
            fixture::tiff(true)
        );
        let structure = report.structure.unwrap();
        assert_eq!(structure.declared_end, Some(bytes.len() as u64));
        assert_eq!(structure.segments[0].kind, "jpeg.soi");
        assert_eq!(structure.segments.last().unwrap().kind, "jpeg.eoi");
        let scans = structure
            .segments
            .iter()
            .filter(|range| range.kind == "jpeg.sos")
            .count();
        assert!(if progressive { scans > 1 } else { scans == 1 });
    }
}

#[test]
fn native_jpeg_parameter_ranges_cross_buffers_and_ignore_embedded_eoi() {
    let mut jpeg = Jpeg::from_bytes(Bytes::from(with_exif(&fixture::tiff(false), true))).unwrap();
    let mut contents = vec![b'Z'; 12_000];
    contents[5000..5002].copy_from_slice(&[0xff, 0xd9]);
    jpeg.segments_mut()
        .insert(0, JpegSegment::new_with_contents(0xe2, contents.into()));
    let bytes = jpeg.encoder().bytes().to_vec();
    let mut codec = Decoder::new(Cursor::new(&bytes));
    assert_eq!(codec.decode().unwrap().len(), 18);
    let report = viewer(bytes.clone());
    assert_eq!(report.exif.latitude, Some(-1.5));
    let block = &report.exif_blocks[0];
    assert_eq!(
        &bytes[block.offset as usize..][..block.length as usize],
        fixture::tiff(false)
    );
    assert_eq!(
        report.structure.unwrap().declared_end,
        Some(bytes.len() as u64)
    );
}

#[test]
fn native_jpeg_eoi_and_budget_failures_cannot_publish_a_complete_end() {
    let bytes = with_exif(&fixture::tiff(true), false);
    let mut truncated = bytes.clone();
    truncated.truncate(truncated.len() - 2);
    let report = viewer(truncated);
    assert!(report.structure.as_ref().unwrap().truncated);
    assert_eq!(report.structure.unwrap().declared_end, None);

    let report = analyze(
        bytes,
        ImageAnalyzeOptions {
            scan_limit: 32,
            ..ImageAnalyzeOptions::viewer()
        },
    );
    assert_eq!(report.status, ImageAnalysisStatus::Oversized);
    assert_eq!(report.structure.unwrap().declared_end, None);
}

#[test]
fn native_jpeg_segment_limit_bounds_real_comment_sequences() {
    let mut jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(BASELINE)).unwrap();
    jpeg.segments_mut().splice(
        0..0,
        (0..8192).map(|_| JpegSegment::new_with_contents(0xfe, Bytes::new())),
    );
    let report = viewer(jpeg.encoder().bytes().to_vec());
    assert_eq!(report.status, ImageAnalysisStatus::Oversized);
    assert_eq!(report.structure.unwrap().declared_end, None);
}

#[test]
fn native_jpeg_frame_errors_are_distinct_from_dimensions_only_and_trailing_data() {
    let bytes = with_exif(&fixture::tiff(true), true);
    let report = viewer(bytes.clone());
    let sof = report
        .structure
        .unwrap()
        .segments
        .into_iter()
        .find(|range| range.kind == "jpeg.sof")
        .unwrap();
    let mut invalid = bytes.clone();
    invalid[sof.offset as usize + 7..sof.offset as usize + 9].copy_from_slice(&[0, 0]);
    assert_eq!(viewer(invalid).status, ImageAnalysisStatus::Corrupt);
    let dimensions = analyze(bytes.clone(), ImageAnalyzeOptions::dimensions());
    assert_eq!(dimensions.dimensions, Some((2, 3)));
    assert!(dimensions.structure.is_none());
    assert!(dimensions.exif_blocks.is_empty());
    let mut trailing = bytes.clone();
    trailing.extend_from_slice(b"tail");
    let report = analyze(trailing, ImageAnalyzeOptions::analysis());
    let structure = report.structure.unwrap();
    assert_eq!(structure.declared_end, Some(bytes.len() as u64));
    assert_eq!(structure.trailing_bytes, Some(4));
    assert_eq!(structure.hashes.trailing_hashed_bytes, Some(4));
}
