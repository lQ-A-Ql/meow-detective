use super::{analyze, fixture, viewer, ImageAnalysisStatus, ImageAnalyzeOptions};
use image_inspection::webp_codec::{
    inspection::{read_chunk_header, WebPRiffChunk},
    ColorType, WebPDecoder, WebPEncoder,
};
use std::io::Cursor;

fn encoded(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    WebPEncoder::new(&mut bytes)
        .encode(
            &vec![123; (width * height * 3) as usize],
            width,
            height,
            ColorType::Rgb8,
        )
        .unwrap();
    bytes
}

#[test]
fn native_webp_headers_match_real_lossless_images_and_dimension_boundaries() {
    for (width, height) in [(2, 3), (16_384, 1), (1, 16_384)] {
        let bytes = encoded(width, height);
        let mut native = WebPDecoder::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(native.dimensions(), (width, height));
        let mut pixels = vec![0; native.output_buffer_size().unwrap()];
        native.read_image(&mut pixels).unwrap();
        assert_eq!(pixels, vec![123; (width * height * 3) as usize]);
        let report = viewer(bytes.clone());
        assert_eq!(report.dimensions, Some((width, height)));
        assert_eq!(report.status, ImageAnalysisStatus::Absent);
        assert_eq!(
            report.structure.unwrap().declared_end,
            Some(bytes.len() as u64)
        );
    }
}

#[test]
fn native_webp_exif_ranges_point_to_the_decoded_source_bytes() {
    for prefixed in [false, true] {
        let tiff = fixture::tiff(false);
        let mut payload = if prefixed {
            b"Exif\0\0".to_vec()
        } else {
            Vec::new()
        };
        payload.extend_from_slice(&tiff);
        let bytes = fixture::webp(&payload);
        let report = viewer(bytes.clone());
        assert_eq!(report.status, ImageAnalysisStatus::Present);
        assert_eq!(report.dimensions, Some((2, 3)));
        assert_eq!(report.exif.latitude, Some(-1.5));
        let block = &report.exif_blocks[0];
        assert_eq!(block.length, tiff.len() as u64);
        assert_eq!(
            &bytes[block.offset as usize..][..block.length as usize],
            &tiff
        );
        let structure = report.structure.unwrap();
        assert_eq!(structure.declared_end, Some(bytes.len() as u64));
        for pair in structure.segments.windows(2) {
            assert_eq!(pair[0].offset + pair[0].length, pair[1].offset);
        }
    }
}

#[test]
fn native_webp_chunk_bounds_reject_incomplete_and_oversized_references() {
    let bytes = encoded(2, 3);
    let mut truncated = bytes.clone();
    truncated.pop();
    let report = viewer(truncated);
    assert!(report.structure.as_ref().unwrap().truncated);
    assert_eq!(report.structure.unwrap().declared_end, None);

    let mut overflow = bytes.clone();
    overflow[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    let (_, length, rounded) = read_chunk_header(Cursor::new(&overflow[12..20])).unwrap();
    assert_eq!(
        (length, rounded),
        (u64::from(u32::MAX), u64::from(u32::MAX) + 1)
    );
    let report = viewer(overflow);
    assert_eq!(report.status, ImageAnalysisStatus::Corrupt);
    assert_eq!(report.structure.unwrap().declared_end, None);

    let mut shortened = bytes.clone();
    shortened[4..8].copy_from_slice(&((bytes.len() - 10) as u32).to_le_bytes());
    let report = viewer(shortened);
    assert_eq!(report.status, ImageAnalysisStatus::Corrupt);
    assert_eq!(report.structure.unwrap().declared_end, None);
}

#[test]
fn native_webp_container_budget_covers_unknown_chunk_sequences() {
    let bytes = encoded(2, 3);
    let (kind, length, _) = read_chunk_header(Cursor::new(&bytes[12..20])).unwrap();
    assert_eq!(kind, WebPRiffChunk::VP8L);
    let mut chunks = vec![riff::ChunkContents::Data(
        riff::ChunkId::new("VP8L").unwrap(),
        bytes[20..20 + length as usize].to_vec(),
    )];
    for _ in 0..8192 {
        chunks.push(riff::ChunkContents::Data(
            riff::ChunkId::new("JUNK").unwrap(),
            Vec::new(),
        ));
    }
    let container =
        riff::ChunkContents::Children(riff::RIFF_ID, riff::ChunkId::new("WEBP").unwrap(), chunks);
    let mut output = Cursor::new(Vec::new());
    container.write(&mut output).unwrap();
    let report = viewer(output.into_inner());
    assert_eq!(report.status, ImageAnalysisStatus::Oversized);
    assert_eq!(report.structure.unwrap().declared_end, None);
}

#[test]
fn real_animated_and_lossy_webp_keep_declared_ranges_and_trailing_hashes() {
    let animated = include_bytes!("../../../fixtures/image-structure/animated.webp");
    let lossy = include_bytes!("../../../fixtures/image-structure/lossy.webp");
    for (bytes, dimensions, expected_frames) in [
        (animated.as_slice(), (2, 3), 2),
        (lossy.as_slice(), (2, 2), 0),
    ] {
        let native = WebPDecoder::new(Cursor::new(bytes)).unwrap();
        assert_eq!(native.dimensions(), dimensions);
        let mut input = bytes.to_vec();
        input.extend_from_slice(b"tail");
        let report = analyze(input, ImageAnalyzeOptions::analysis());
        assert_eq!(report.dimensions, Some(dimensions));
        let structure = report.structure.unwrap();
        assert_eq!(structure.declared_end, Some(bytes.len() as u64));
        assert_eq!(structure.trailing_bytes, Some(4));
        assert_eq!(structure.hashes.trailing_hashed_bytes, Some(4));
        assert_eq!(
            structure
                .segments
                .iter()
                .filter(|range| range.kind == "webp.ANMF")
                .count(),
            expected_frames
        );
    }
}
