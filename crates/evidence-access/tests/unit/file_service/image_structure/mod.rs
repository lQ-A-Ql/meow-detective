#[path = "../image_metadata_fixture.rs"]
mod fixture;
mod gif_native;
mod ico_native;
mod jpeg_native;
mod tiff_native;
mod webp_native;

use std::sync::atomic::AtomicBool;

use crate::file_service::FileServiceError;

use super::{
    analyze_image, ImageAnalysisStatus, ImageAnalyzeOptions, ImageFormat, ImageStructureError,
    SeekableSource,
};

fn analyze(bytes: Vec<u8>, options: ImageAnalyzeOptions) -> super::ImageAnalysisReport {
    let mut source = SeekableSource::new(std::io::Cursor::new(bytes)).unwrap();
    analyze_image(&mut source, &options, &AtomicBool::new(false)).expect("analysis reads")
}

fn viewer(bytes: Vec<u8>) -> super::ImageAnalysisReport {
    analyze(bytes, ImageAnalyzeOptions::viewer())
}

fn gif() -> Vec<u8> {
    gif_native::encoded(false)
}

fn bmp() -> Vec<u8> {
    let mut bytes = vec![0u8; 78];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&78u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&2i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&3i32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    bytes
}

fn ico() -> Vec<u8> {
    ico_native::encoded(&[(2, 3)])
}

fn jpeg_with_frame() -> Vec<u8> {
    jpeg_native::BASELINE.to_vec()
}

fn png_with_two_exif(tiff: &[u8]) -> Vec<u8> {
    use image_inspection::png_codec::{chunk, BitDepth, ColorType, Encoder};
    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, 2, 3);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_chunk(chunk::eXIf, tiff).unwrap();
        writer.write_chunk(chunk::eXIf, tiff).unwrap();
        writer.write_image_data(&[0u8; 18]).unwrap();
        writer.finish().unwrap();
    }
    bytes
}

#[test]
fn detects_and_walks_every_supported_container() {
    let tiff = viewer(fixture::tiff(true));
    assert_eq!(tiff.format, Some(ImageFormat::Tiff));
    assert_eq!(tiff.status, ImageAnalysisStatus::Present);
    assert_eq!(tiff.exif.latitude, Some(-1.5));
    assert_eq!(tiff.exif.longitude, Some(-2.0));

    let jpeg = viewer(fixture::jpeg(&fixture::tiff(true)));
    assert_eq!(jpeg.format, Some(ImageFormat::Jpeg));
    assert_eq!(jpeg.status, ImageAnalysisStatus::Present);
    assert_eq!(
        jpeg.exif.date_time_original.as_deref(),
        Some("2026:10:14 12:34:56")
    );

    let png = viewer(fixture::png(Some(&fixture::tiff(true))));
    assert_eq!(png.format, Some(ImageFormat::Png));
    assert_eq!(png.dimensions, Some((2, 3)));
    assert_eq!(png.status, ImageAnalysisStatus::Present);

    let webp = viewer(fixture::webp(&fixture::tiff(false)));
    assert_eq!(webp.format, Some(ImageFormat::Webp));
    assert_eq!(webp.status, ImageAnalysisStatus::Present);

    let framed = viewer(jpeg_with_frame());
    assert_eq!(framed.dimensions, Some((2, 3)));
    assert_eq!(framed.status, ImageAnalysisStatus::Absent);

    let gif = viewer(gif());
    assert_eq!(gif.format, Some(ImageFormat::Gif));
    assert_eq!(gif.dimensions, Some((2, 3)));
    assert_eq!(gif.status, ImageAnalysisStatus::Absent);

    let bmp = viewer(bmp());
    assert_eq!(bmp.format, Some(ImageFormat::Bmp));
    assert_eq!(bmp.dimensions, Some((2, 3)));

    let ico = viewer(ico());
    assert_eq!(ico.format, Some(ImageFormat::Ico));
    assert_eq!(ico.dimensions, Some((2, 3)));
}

#[test]
fn records_exif_blocks_and_declared_container_end() {
    let bytes = fixture::tiff(true);
    let expected_end = bytes.len() as u64 - 37;
    let tiff = viewer(bytes);
    assert_eq!(tiff.exif_blocks.len(), 1);
    assert_eq!(tiff.exif_blocks[0].source, "tiff.ifd0");
    assert!(tiff.exif_blocks[0].decoded);
    assert_eq!(tiff.exif_blocks[0].field_count, 8);
    let structure = tiff.structure.expect("structure report");
    assert_eq!(structure.declared_end, Some(expected_end));
    assert_eq!(structure.trailing_bytes, Some(37));
    assert!(structure.findings.iter().any(|finding| {
        finding.code == "image.trailing-data" && finding.severity == super::ImageIssueSeverity::Info
    }));

    let png_bytes = fixture::png(Some(&fixture::tiff(true)));
    let png = viewer(png_bytes.clone());
    assert_eq!(png.exif_blocks.len(), 1);
    assert_eq!(png.exif_blocks[0].source, "png.exif");
    let structure = png.structure.expect("structure report");
    assert_eq!(structure.declared_end, Some(png_bytes.len() as u64));
    assert_eq!(structure.trailing_bytes, Some(0));
}

#[test]
fn trailing_bytes_are_hashed_only_in_analysis_mode() {
    let mut bytes = fixture::png(Some(&fixture::tiff(true)));
    bytes.extend_from_slice(b"tail");

    let viewer_report = viewer(bytes.clone());
    let structure = viewer_report.structure.as_ref().expect("structure report");
    assert_eq!(structure.trailing_bytes, Some(4));
    assert!(structure.hashes.whole_file_sha256.is_none());
    assert!(structure.hashes.trailing_sha256.is_none());

    let analysis = analyze(bytes, ImageAnalyzeOptions::analysis());
    let structure = analysis.structure.as_ref().expect("structure report");
    let expected = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(b"tail"))
    };
    assert_eq!(
        structure.hashes.trailing_sha256.as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(structure.hashes.trailing_hashed_bytes, Some(4));
    assert!(structure.hashes.whole_file_sha256.is_some());
}

#[test]
fn truncation_is_reported_without_hashing_a_complete_container() {
    let mut bytes = fixture::jpeg(&fixture::tiff(true));
    bytes.truncate(bytes.len() - 2);
    let report = viewer(bytes);
    let structure = report.structure.expect("structure report");
    assert!(structure.truncated);
    assert_eq!(structure.declared_end, None);
    assert!(structure
        .findings
        .iter()
        .any(|finding| finding.severity == super::ImageIssueSeverity::Error));
}

#[test]
fn corrupt_exif_keeps_container_structure_and_reports_corrupt() {
    let mut tiff = fixture::tiff(true);
    tiff[8..10].copy_from_slice(&100u16.to_le_bytes());
    let report = viewer(tiff);
    assert_eq!(report.status, ImageAnalysisStatus::Corrupt);
    assert!(report.exif_blocks.is_empty());
    assert!(report.exif.latitude.is_none());
    assert!(report.structure.is_some());
}

#[test]
fn dimensions_only_skips_exif_and_structure_work() {
    let mut bytes = fixture::png(Some(&fixture::tiff(true)));
    bytes.extend_from_slice(b"tail");
    let report = analyze(bytes, ImageAnalyzeOptions::dimensions());
    assert_eq!(report.dimensions, Some((2, 3)));
    assert_eq!(report.status, ImageAnalysisStatus::Absent);
    assert!(report.exif_blocks.is_empty());
    assert!(report.exif.is_empty());
    assert!(report.structure.is_none());
    assert!(report.warnings.is_empty());
}

#[test]
fn unsupported_payload_is_distinct_from_a_corrupt_container() {
    let report = viewer(b"plain text".to_vec());
    assert_eq!(report.status, ImageAnalysisStatus::Unsupported);
    assert_eq!(report.format, None);
    assert!(report.structure.is_none());
}

#[test]
fn multiple_exif_blocks_are_reported_once() {
    let tiff = fixture::tiff(true);
    let report = viewer(png_with_two_exif(&tiff));
    assert_eq!(report.exif_blocks.len(), 2);
    assert_eq!(report.status, ImageAnalysisStatus::Present);
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.code == "image.exif.multiple-blocks"));
}

#[test]
fn invalid_checksum_keeps_dimensions_and_warns() {
    let mut bytes = fixture::png(Some(&fixture::tiff(true)));
    let kind = bytes
        .windows(4)
        .position(|window| window == b"eXIf")
        .expect("eXIf chunk");
    let chunk_start = kind - 4;
    let payload_length = u32::from_be_bytes(
        bytes[chunk_start..chunk_start + 4]
            .try_into()
            .expect("chunk length"),
    ) as usize;
    let crc_start = chunk_start + 8 + payload_length;
    bytes[crc_start] ^= 0xff;
    let report = viewer(bytes);
    assert_eq!(report.dimensions, Some((2, 3)));
    assert_eq!(report.status, ImageAnalysisStatus::Absent);
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.code == "image.png.crc-invalid"));
}

#[test]
fn bmp_size_mismatch_is_a_structure_finding() {
    let mut bytes = bmp();
    bytes[2..6].copy_from_slice(&60u32.to_le_bytes());
    let report = viewer(bytes);
    assert_eq!(report.dimensions, Some((2, 3)));
    let structure = report.structure.expect("structure report");
    assert_eq!(structure.declared_end, Some(60));
    assert_eq!(structure.trailing_bytes, Some(18));
    assert!(structure
        .findings
        .iter()
        .any(|finding| finding.code == "image.bmp.size-mismatch"));
}

#[test]
fn source_errors_and_cancellation_propagate() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.png");
    std::fs::write(&path, fixture::png(None)).unwrap();
    let mut source = SeekableSource::new(std::fs::File::open(&path).unwrap()).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(0)
        .unwrap();
    let error = analyze_image(
        &mut source,
        &ImageAnalyzeOptions::viewer(),
        &AtomicBool::new(false),
    )
    .expect_err("read errors propagate");
    assert!(matches!(
        error,
        ImageStructureError::Read(FileServiceError::Io(_))
    ));

    let mut source = SeekableSource::new(std::io::Cursor::new(fixture::png(None))).unwrap();
    let cancelled = analyze_image(
        &mut source,
        &ImageAnalyzeOptions::viewer(),
        &AtomicBool::new(true),
    );
    assert!(matches!(cancelled, Err(ImageStructureError::Cancelled)));
}

#[test]
fn native_png_ranges_remain_exact_across_input_buffer_boundaries() {
    let mut tiff = fixture::tiff(true);
    tiff.resize(70_000, 0);
    let bytes = fixture::png(Some(&tiff));
    let report = viewer(bytes.clone());
    assert_eq!(report.status, ImageAnalysisStatus::Present);
    let structure = report.structure.unwrap();
    assert_eq!(structure.declared_end, Some(bytes.len() as u64));
    assert_eq!(structure.trailing_bytes, Some(0));
    assert_eq!(structure.segments.first().unwrap().offset, 8);
    for pair in structure.segments.windows(2) {
        assert_eq!(pair[0].offset + pair[0].length, pair[1].offset);
    }
    let last = structure.segments.last().unwrap();
    assert_eq!(last.kind, "png.IEND");
    assert_eq!(last.offset + last.length, bytes.len() as u64);
}

#[test]
fn native_png_scan_budget_cannot_publish_a_complete_structure() {
    let mut tiff = fixture::tiff(true);
    tiff.resize(70_000, 0);
    let report = analyze(
        fixture::png(Some(&tiff)),
        ImageAnalyzeOptions {
            scan_limit: 64,
            ..ImageAnalyzeOptions::viewer()
        },
    );
    assert_eq!(report.status, ImageAnalysisStatus::Oversized);
    assert_eq!(report.structure.unwrap().declared_end, None);
    assert!(report.bytes_read <= 80);
}

#[test]
fn native_png_structural_decoder_rejects_a_damaged_idat_checksum() {
    let mut bytes = fixture::png(None);
    let parsed = viewer(bytes.clone());
    let structure = parsed.structure.unwrap();
    let idat = structure
        .segments
        .iter()
        .find(|segment| segment.kind == "png.IDAT")
        .unwrap();
    bytes[idat.offset as usize + 8] ^= 1;
    let damaged = viewer(bytes);
    assert_eq!(damaged.status, ImageAnalysisStatus::Corrupt);
    assert_eq!(damaged.structure.unwrap().declared_end, None);
}
