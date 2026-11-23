use super::{analyze, fixture, viewer, ImageAnalysisStatus, ImageAnalyzeOptions};
use exif::{experimental::Writer, Context, Field, In, ParseLimits, Tag, Value};
use std::io::Cursor;

fn encode(fields: &[Field], little: bool) -> Vec<u8> {
    let mut writer = Writer::new();
    for field in fields {
        writer.push_field(field);
    }
    let mut output = Cursor::new(Vec::new());
    writer.write(&mut output, little).unwrap();
    output.into_inner()
}

fn field(tag: Tag, value: Value) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value,
    }
}

#[test]
fn native_exif_rejects_entry_and_value_limits_before_materializing_fields() {
    let fields = (0..2049)
        .map(|index| field(Tag(Context::Tiff, 0xc000 + index), Value::Long(vec![1])))
        .collect::<Vec<_>>();
    let too_many = viewer(fixture::png(Some(&encode(&fields, true))));
    assert_eq!(too_many.status, ImageAnalysisStatus::Oversized);
    assert!(!too_many.exif_blocks[0].decoded);
    assert_eq!(too_many.exif_blocks[0].warnings[0].code, "image.exif.limit");

    for (length, expected) in [
        (4095, ImageAnalysisStatus::Present),
        (4096, ImageAnalysisStatus::Oversized),
    ] {
        let fields = [field(Tag::Make, Value::Ascii(vec![vec![b'A'; length]]))];
        let report = viewer(fixture::png(Some(&encode(&fields, false))));
        assert_eq!(report.status, expected);
    }
}

#[test]
fn native_parser_limits_are_fatal_in_continue_on_error_mode() {
    let bytes = encode(
        &[
            field(Tag::Orientation, Value::Short(vec![1])),
            field(Tag::ImageWidth, Value::Long(vec![2])),
        ],
        true,
    );
    let error = exif::Reader::new()
        .continue_on_error(true)
        .limits(ParseLimits {
            max_total_value_bytes: 5,
            ..Default::default()
        })
        .read_raw(bytes)
        .err()
        .expect("total value budget must fail");
    assert!(matches!(error, exif::Error::TooBig(_)));
    let error = exif::Reader::new()
        .limits(ParseLimits {
            max_directories: 1,
            ..Default::default()
        })
        .read_raw(fixture::tiff(true))
        .err()
        .expect("directory budget must fail");
    assert!(matches!(error, exif::Error::TooBig(_)));
}

#[test]
fn native_exif_rejects_directory_cycles_without_losing_png_structure() {
    let mut tiff = fixture::tiff(true);
    tiff[46..50].copy_from_slice(&8u32.to_le_bytes());
    let png = fixture::png(Some(&tiff));
    let expected_end = png.len() as u64;
    let report = viewer(png);
    assert_eq!(report.status, ImageAnalysisStatus::Corrupt);
    assert!(!report.exif_blocks[0].decoded);
    assert_eq!(report.structure.unwrap().declared_end, Some(expected_end));
}

#[test]
fn exif_block_input_bound_is_enforced_for_native_decode() {
    let mut tiff = fixture::tiff(true);
    tiff.resize(1024 * 1024 + 1, 0);
    let report = viewer(fixture::png(Some(&tiff)));
    assert_eq!(report.status, ImageAnalysisStatus::Oversized);
    assert!(!report.exif_blocks[0].decoded);
    assert_eq!(
        report.exif_blocks[0].warnings[0].code,
        "image.exif.block-too-large"
    );
}

#[test]
fn repeated_native_exif_blocks_stop_at_the_report_limit() {
    use image_inspection::png_codec::{chunk, Encoder};
    let tiff = fixture::tiff(true);
    let mut bytes = Vec::new();
    {
        let mut writer = Encoder::new(&mut bytes, 1, 1).write_header().unwrap();
        for _ in 0..17 {
            writer.write_chunk(chunk::eXIf, &tiff).unwrap();
        }
        writer.write_image_data(&[0]).unwrap();
        writer.finish().unwrap();
    }
    let report = viewer(bytes);
    assert_eq!(report.status, ImageAnalysisStatus::Present);
    assert_eq!(report.exif_blocks.len(), 16);
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.code == "image.exif.block-limit"));
}

#[test]
fn native_tiff_raster_ranges_determine_the_container_end() {
    for little in [true, false] {
        for tiled in [false, true] {
            let fields = [
                field(Tag::ImageWidth, Value::Long(vec![2])),
                field(Tag::ImageLength, Value::Long(vec![3])),
                field(Tag::BitsPerSample, Value::Short(vec![8])),
                field(Tag::Compression, Value::Short(vec![1])),
                field(Tag::PhotometricInterpretation, Value::Short(vec![1])),
                field(Tag::SamplesPerPixel, Value::Short(vec![1])),
                field(Tag::RowsPerStrip, Value::Long(vec![3])),
                field(Tag(Context::Tiff, 0x0142), Value::Long(vec![16])),
                field(Tag(Context::Tiff, 0x0143), Value::Long(vec![16])),
            ];
            let pixels = vec![0x55; if tiled { 256 } else { 6 }];
            let payloads = [pixels.as_slice()];
            let mut writer = Writer::new();
            for field in &fields {
                writer.push_field(field);
            }
            if tiled {
                writer.set_tiles(&payloads, In::PRIMARY);
            } else {
                writer.set_strips(&payloads, In::PRIMARY);
            }
            let mut output = Cursor::new(Vec::new());
            writer.write(&mut output, little).unwrap();
            let mut bytes = output.into_inner();
            let expected_end = bytes.len() as u64;
            bytes.extend_from_slice(b"tail");
            let report = viewer(bytes.clone());
            assert_eq!(report.dimensions, Some((2, 3)));
            let structure = report.structure.unwrap();
            assert_eq!(structure.declared_end, Some(expected_end));
            assert_eq!(structure.trailing_bytes, Some(4));
            bytes.truncate(expected_end as usize - 1);
            let truncated = viewer(bytes);
            let structure = truncated.structure.unwrap();
            assert!(structure.truncated);
            assert_eq!(structure.declared_end, None);
        }
    }
}

#[test]
fn bounded_native_tiff_scan_does_not_publish_an_unverified_end() {
    let mut tiff = fixture::tiff(true);
    tiff.resize(4000, 0);
    let report = analyze(
        tiff,
        ImageAnalyzeOptions {
            random_read_limit: 64,
            ..ImageAnalyzeOptions::viewer()
        },
    );
    assert_eq!(report.status, ImageAnalysisStatus::Oversized);
    assert_eq!(report.structure.unwrap().declared_end, None);
    assert_eq!(report.bytes_read, 64);
}
