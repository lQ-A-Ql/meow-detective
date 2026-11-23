use super::{analyze, viewer, ImageAnalysisStatus, ImageAnalyzeOptions};
use image_inspection::gif_codec::{AnyExtension, Encoder, Frame, Repeat};
use std::borrow::Cow;

pub(super) fn encoded(large_comment: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    let palette = [0, 0, 0, 255, 255, 255];
    {
        let mut encoder = Encoder::new(&mut bytes, 2, 3, &palette).unwrap();
        encoder.set_repeat(Repeat::Infinite).unwrap();
        if large_comment {
            encoder
                .write_raw_extension(AnyExtension(0xfe), &[&vec![b'A'; 70_000]])
                .unwrap();
        }
        for local_palette in [false, true] {
            encoder
                .write_frame(&Frame {
                    width: 2,
                    height: 3,
                    interlaced: local_palette,
                    palette: local_palette.then(|| palette.to_vec()),
                    buffer: Cow::Borrowed(&[0, 1, 0, 1, 1, 0]),
                    ..Frame::default()
                })
                .unwrap();
        }
        encoder.into_inner().unwrap();
    }
    bytes
}

#[test]
fn native_gif_events_cover_palette_extensions_and_all_frames_exactly() {
    for large_comment in [false, true] {
        let bytes = encoded(large_comment);
        let expected_end = bytes.len() as u64;
        let report = viewer(bytes);
        assert_eq!(report.status, ImageAnalysisStatus::Absent);
        assert_eq!(report.dimensions, Some((2, 3)));
        let structure = report.structure.unwrap();
        assert_eq!(structure.declared_end, Some(expected_end));
        assert_eq!(structure.trailing_bytes, Some(0));
        assert_eq!(structure.segments[0].offset, 0);
        assert_eq!(structure.segments[0].kind, "gif.header");
        for pair in structure.segments.windows(2) {
            assert_eq!(pair[0].offset + pair[0].length, pair[1].offset);
        }
        assert_eq!(
            structure
                .segments
                .iter()
                .filter(|range| range.kind == "gif.image")
                .count(),
            2
        );
        assert_eq!(structure.segments.last().unwrap().kind, "gif.trailer");
        assert_eq!(structure.segments.last().unwrap().offset + 1, expected_end);
    }
}

#[test]
fn native_gif_missing_trailer_and_scan_limit_never_publish_a_complete_end() {
    let mut bytes = encoded(false);
    bytes.pop();
    let truncated = viewer(bytes);
    assert!(truncated.structure.as_ref().unwrap().truncated);
    assert_eq!(truncated.structure.unwrap().declared_end, None);

    let bounded = analyze(
        encoded(true),
        ImageAnalyzeOptions {
            scan_limit: 64,
            ..ImageAnalyzeOptions::viewer()
        },
    );
    assert_eq!(bounded.status, ImageAnalysisStatus::Oversized);
    assert_eq!(bounded.structure.unwrap().declared_end, None);
    assert!(bounded.bytes_read <= 80);
}

#[test]
fn native_gif_dimensions_and_trailing_hash_do_not_require_pixel_decoding() {
    let mut bytes = encoded(true);
    let expected_end = bytes.len() as u64;
    bytes.extend_from_slice(b"trailing");
    let dimensions = analyze(bytes.clone(), ImageAnalyzeOptions::dimensions());
    assert_eq!(dimensions.dimensions, Some((2, 3)));
    assert!(dimensions.structure.is_none());
    assert_eq!(dimensions.bytes_read, 29);

    let analyzed = analyze(bytes, ImageAnalyzeOptions::analysis());
    let structure = analyzed.structure.unwrap();
    assert_eq!(structure.declared_end, Some(expected_end));
    assert_eq!(structure.trailing_bytes, Some(8));
    assert_eq!(structure.hashes.trailing_hashed_bytes, Some(8));
}

#[test]
fn native_gif_preserves_unknown_extensions_and_rejects_unknown_blocks() {
    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, 2, 3, &[0, 0, 0, 255, 255, 255]).unwrap();
        encoder
            .write_raw_extension(AnyExtension(0xee), &[b"metadata"])
            .unwrap();
        encoder
            .write_frame(&Frame {
                width: 2,
                height: 3,
                buffer: Cow::Borrowed(&[0; 6]),
                ..Frame::default()
            })
            .unwrap();
        encoder.into_inner().unwrap();
    }
    let report = viewer(bytes.clone());
    let structure = report.structure.unwrap();
    let extension = structure
        .segments
        .iter()
        .find(|range| range.kind == "gif.extension.ee")
        .unwrap();
    assert_eq!(report.status, ImageAnalysisStatus::Absent);
    assert_eq!(structure.declared_end, Some(bytes.len() as u64));
    bytes[extension.offset as usize] = 0x22;
    let invalid = viewer(bytes);
    assert_eq!(invalid.status, ImageAnalysisStatus::Corrupt);
    assert_eq!(invalid.structure.unwrap().declared_end, None);
}

#[test]
fn native_gif_event_limits_bound_empty_extensions_and_sub_blocks() {
    for sub_blocks in [false, true] {
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, 1, 1, &[0, 0, 0]).unwrap();
            if sub_blocks {
                let parts = vec![b"A".as_slice(); 16_385];
                encoder
                    .write_raw_extension(AnyExtension(0xfe), &parts)
                    .unwrap();
            } else {
                for _ in 0..8193 {
                    encoder
                        .write_raw_extension(AnyExtension(0xfe), &[])
                        .unwrap();
                }
            }
            encoder
                .write_frame(&Frame {
                    width: 1,
                    height: 1,
                    buffer: Cow::Borrowed(&[0]),
                    ..Frame::default()
                })
                .unwrap();
            encoder.into_inner().unwrap();
        }
        let report = viewer(bytes);
        assert_eq!(report.status, ImageAnalysisStatus::Oversized);
        assert_eq!(report.structure.unwrap().declared_end, None);
    }
}
