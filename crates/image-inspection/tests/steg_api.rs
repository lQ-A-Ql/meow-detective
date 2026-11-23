use image_inspection::{analyze, PlaneKind, StegBand, StegFormat, StegLimits, StegStatus};

fn encode_png(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    palette: Option<Vec<u8>>,
    data: &[u8],
) -> Vec<u8> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if let Some(palette) = palette {
            encoder.set_palette(palette);
        }
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(data).expect("png data");
        writer.finish().expect("png finish");
    }
    output
}

fn rgb_gradient(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            data.push(((x * 3 + y * 5) % 256) as u8);
            data.push(((x * 7 + y * 11 + 17) % 256) as u8);
            data.push(((x * 13 + y * 2 + 91) % 256) as u8);
        }
    }
    data
}

fn natural_cover(width: u32, height: u32) -> Vec<u8> {
    let mut state = 0x1234_5678_9ABC_DEF0u64;
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let base = 128.0 + 34.0 * (x as f32 / 17.0).sin() + 34.0 * (y as f32 / 13.0).cos();
            for channel in 0..3u32 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let noise = ((state >> 33) % 21) as i32 - 10;
                let value = (base as i32 + noise + channel as i32 * 5).clamp(1, 254);
                data.push((value & !1) as u8);
            }
        }
    }
    data
}

fn embed_lsb(data: &mut [u8], percent: u64) {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for value in data {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        if state % 100 < percent {
            *value = (*value & 0xFE) | (state >> 63) as u8;
        }
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn with_interlaced_flag(mut png: Vec<u8>) -> Vec<u8> {
    assert_eq!(&png[12..16], b"IHDR");
    png[28] = 1;
    let crc = crc32(&png[12..29]);
    png[29..33].copy_from_slice(&crc.to_be_bytes());
    png
}

fn encode_bmp(width: u32, height: u32, bits: u16, top_down: bool, logical: &[u8]) -> Vec<u8> {
    let bytes_per_pixel = (bits / 8) as usize;
    let row_bytes = width as usize * bytes_per_pixel;
    let row_stride = row_bytes.div_ceil(4) * 4;
    let palette_bytes = if bits == 8 { 256 * 4 } else { 0 };
    let data_offset = 54 + palette_bytes;
    let image_size = row_stride * height as usize;
    let mut output = vec![0u8; data_offset + image_size];
    output[0..2].copy_from_slice(b"BM");
    output[2..6].copy_from_slice(&((data_offset + image_size) as u32).to_le_bytes());
    output[10..14].copy_from_slice(&(data_offset as u32).to_le_bytes());
    output[14..18].copy_from_slice(&40u32.to_le_bytes());
    output[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    let height_field: i32 = if top_down {
        -(height as i32)
    } else {
        height as i32
    };
    output[22..26].copy_from_slice(&height_field.to_le_bytes());
    output[26..28].copy_from_slice(&1u16.to_le_bytes());
    output[28..30].copy_from_slice(&bits.to_le_bytes());
    output[34..38].copy_from_slice(&(image_size as u32).to_le_bytes());
    if bits == 8 {
        output[46..50].copy_from_slice(&256u32.to_le_bytes());
        for index in 0..256usize {
            let offset = 54 + index * 4;
            output[offset] = index as u8;
            output[offset + 1] = index as u8;
            output[offset + 2] = index as u8;
        }
    }
    for y in 0..height as usize {
        let destination_row = if top_down { y } else { height as usize - 1 - y };
        let destination = data_offset + destination_row * row_stride;
        let source = y * row_bytes;
        output[destination..destination + row_bytes]
            .copy_from_slice(&logical[source..source + row_bytes]);
    }
    output
}

fn bmp_header_only(bits: u16, compression: u32) -> Vec<u8> {
    let mut output = vec![0u8; 54];
    output[0..2].copy_from_slice(b"BM");
    output[2..6].copy_from_slice(&54u32.to_le_bytes());
    output[10..14].copy_from_slice(&54u32.to_le_bytes());
    output[14..18].copy_from_slice(&40u32.to_le_bytes());
    output[18..22].copy_from_slice(&32i32.to_le_bytes());
    output[22..26].copy_from_slice(&32i32.to_le_bytes());
    output[26..28].copy_from_slice(&1u16.to_le_bytes());
    output[28..30].copy_from_slice(&bits.to_le_bytes());
    output[30..34].copy_from_slice(&compression.to_le_bytes());
    output
}

fn plane_kinds(report: &image_inspection::StegReport) -> Vec<PlaneKind> {
    report.planes.iter().map(|plane| plane.plane).collect()
}

fn has_warning(report: &image_inspection::StegReport, code: &str) -> bool {
    report.warnings.iter().any(|warning| warning.code == code)
}

#[test]
fn png_rgb_reports_three_planes() {
    let png = encode_png(
        64,
        64,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &rgb_gradient(64, 64),
    );
    let report = analyze(&png, StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Completed);
    assert_eq!(report.format, Some(StegFormat::Png));
    assert_eq!((report.width, report.height), (Some(64), Some(64)));
    assert_eq!(
        plane_kinds(&report),
        vec![PlaneKind::Red, PlaneKind::Green, PlaneKind::Blue]
    );
    assert!(report.planes.iter().all(|plane| plane.samples == 4096));
    assert!(report.planes.iter().all(|plane| plane.unique_values > 2));
    assert_eq!(report.blocks_total, 1);
    assert!(report.blocks_scored <= 1);
    assert!(!has_warning(&report, "image.steg.alpha-excluded"));
    assert!(report.reason.is_none());
}

#[test]
fn png_rgba_and_gray_alpha_exclude_alpha() {
    let mut rgba = Vec::new();
    for index in 0..64 * 64u32 {
        rgba.extend_from_slice(&[(index % 256) as u8, (index / 3) as u8, 120, 255]);
    }
    let png = encode_png(
        64,
        64,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        None,
        &rgba,
    );
    let report = analyze(&png, StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Completed);
    assert_eq!(
        plane_kinds(&report),
        vec![PlaneKind::Red, PlaneKind::Green, PlaneKind::Blue]
    );
    assert!(has_warning(&report, "image.steg.alpha-excluded"));

    let mut gray_alpha = Vec::new();
    for index in 0..64 * 64u32 {
        gray_alpha.extend_from_slice(&[(index % 256) as u8, 255]);
    }
    let png = encode_png(
        64,
        64,
        png::ColorType::GrayscaleAlpha,
        png::BitDepth::Eight,
        None,
        &gray_alpha,
    );
    let report = analyze(&png, StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Completed);
    assert_eq!(plane_kinds(&report), vec![PlaneKind::Gray]);
    assert!(has_warning(&report, "image.steg.alpha-excluded"));
}

#[test]
fn png_indexed_is_index_only() {
    let mut indices = Vec::with_capacity(64 * 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            indices.push(((x * 4 + y) % 256) as u8);
        }
    }
    let palette: Vec<u8> = (0..=255u8)
        .flat_map(|value| [value, value, value])
        .collect();
    let png = encode_png(
        64,
        64,
        png::ColorType::Indexed,
        png::BitDepth::Eight,
        Some(palette),
        &indices,
    );
    let report = analyze(&png, StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Completed);
    assert_eq!(plane_kinds(&report), vec![PlaneKind::Index]);
    assert_eq!(report.dominant_plane, Some(PlaneKind::Index));
    assert!(has_warning(&report, "image.steg.palette-index-domain"));
    assert!(has_warning(&report, "image.steg.single-method"));
    let global = report.global.expect("index plane assessment");
    assert_eq!(global.band, StegBand::Inconclusive);
    assert!(global.score.is_none());
    assert_eq!(global.methods_used, 1);
}

#[test]
fn png_16_bit_is_unsupported() {
    let data = vec![0u8; 64 * 64 * 3 * 2];
    let png = encode_png(
        64,
        64,
        png::ColorType::Rgb,
        png::BitDepth::Sixteen,
        None,
        &data,
    );
    let report = analyze(&png, StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Unsupported);
    assert_eq!(report.reason, Some("png.bit-depth"));
    assert_eq!((report.width, report.height), (Some(64), Some(64)));
}

#[test]
fn png_interlaced_is_unsupported() {
    let png = encode_png(
        64,
        64,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &rgb_gradient(64, 64),
    );
    let report = analyze(
        &with_interlaced_flag(png),
        StegFormat::Png,
        &StegLimits::default(),
    );
    assert_eq!(report.status, StegStatus::Unsupported);
    assert_eq!(report.reason, Some("png.interlaced"));
}

#[test]
fn png_apng_is_unsupported() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 64, 64);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(1, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgb_gradient(64, 64)).unwrap();
        writer.finish().unwrap();
    }
    let report = analyze(&bytes, StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Unsupported);
    assert_eq!(report.reason, Some("png.apng"));
}

#[test]
fn corrupt_inputs_are_reported() {
    let report = analyze(b"not a png", StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Corrupt);

    let png = encode_png(
        64,
        64,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &rgb_gradient(64, 64),
    );
    let truncated = &png[..png.len() / 2];
    let report = analyze(truncated, StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Corrupt);

    let report = analyze(&[0u8; 20], StegFormat::Bmp, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Corrupt);
    assert_eq!(report.reason, Some("bmp.header"));
}

#[test]
fn png_tiny_image_skips_statistics() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &[10, 20, 30],
    );
    let report = analyze(&png, StegFormat::Png, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Completed);
    assert!(report.global.is_none());
    assert!(report.planes.iter().all(|plane| plane.samples == 1));
    assert!(report.planes.iter().all(|plane| plane.assessment.is_none()));
    assert!(has_warning(&report, "image.steg.small-image"));
    assert_eq!(report.blocks_total, 1);
    assert_eq!(report.blocks_scored, 0);
}

#[test]
fn bmp_24_bit_reports_rgb() {
    let mut bgr = Vec::with_capacity(64 * 64 * 3);
    for y in 0..64u32 {
        for x in 0..64u32 {
            bgr.push(((x * 3 + y * 7) % 256) as u8);
            bgr.push(((x * 5 + y * 11) % 256) as u8);
            bgr.push(((x * 13 + y * 17) % 256) as u8);
        }
    }
    let bmp = encode_bmp(64, 64, 24, true, &bgr);
    let report = analyze(&bmp, StegFormat::Bmp, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Completed);
    assert_eq!(report.format, Some(StegFormat::Bmp));
    assert_eq!(
        plane_kinds(&report),
        vec![PlaneKind::Red, PlaneKind::Green, PlaneKind::Blue]
    );
    assert!(report.planes.iter().all(|plane| plane.samples == 4096));
}

#[test]
fn bmp_32_bit_excludes_alpha() {
    let mut bgra = Vec::with_capacity(64 * 64 * 4);
    for index in 0..64 * 64u32 {
        bgra.extend_from_slice(&[(index % 256) as u8, (index / 5) as u8, 80, 255]);
    }
    let bmp = encode_bmp(64, 64, 32, true, &bgra);
    let report = analyze(&bmp, StegFormat::Bmp, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Completed);
    assert_eq!(
        plane_kinds(&report),
        vec![PlaneKind::Red, PlaneKind::Green, PlaneKind::Blue]
    );
    assert!(has_warning(&report, "image.steg.alpha-excluded"));
}

#[test]
fn bmp_8_bit_is_index_only() {
    let mut indices = Vec::with_capacity(64 * 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            indices.push(((x * 4 + y) % 256) as u8);
        }
    }
    let bmp = encode_bmp(64, 64, 8, true, &indices);
    let report = analyze(&bmp, StegFormat::Bmp, &StegLimits::default());
    assert_eq!(report.status, StegStatus::Completed);
    assert_eq!(plane_kinds(&report), vec![PlaneKind::Index]);
    assert!(has_warning(&report, "image.steg.palette-index-domain"));
    assert_eq!(report.dominant_plane, Some(PlaneKind::Index));
}

#[test]
fn bmp_row_order_does_not_change_statistics() {
    let mut bgr = Vec::with_capacity(61 * 61 * 3);
    for y in 0..61u32 {
        for x in 0..61u32 {
            bgr.push(((x * 3 + y * 7) % 256) as u8);
            bgr.push(((x * 5 + y * 11) % 256) as u8);
            bgr.push(((x * 13 + y * 17) % 256) as u8);
        }
    }
    let top_down = analyze(
        &encode_bmp(61, 61, 24, true, &bgr),
        StegFormat::Bmp,
        &StegLimits::default(),
    );
    let bottom_up = analyze(
        &encode_bmp(61, 61, 24, false, &bgr),
        StegFormat::Bmp,
        &StegLimits::default(),
    );
    assert_eq!(top_down.status, StegStatus::Completed);
    assert_eq!(bottom_up.status, StegStatus::Completed);
    assert_eq!(top_down.planes.len(), bottom_up.planes.len());
    for (left, right) in top_down.planes.iter().zip(bottom_up.planes.iter()) {
        assert_eq!(left.samples, right.samples);
        assert_eq!(left.unique_values, right.unique_values);
    }
    assert_eq!(
        top_down.global.as_ref().and_then(|value| value.score),
        bottom_up.global.as_ref().and_then(|value| value.score)
    );
}

#[test]
fn bmp_unsupported_variants_are_typed() {
    let compressed = analyze(
        &bmp_header_only(24, 3),
        StegFormat::Bmp,
        &StegLimits::default(),
    );
    assert_eq!(compressed.status, StegStatus::Unsupported);
    assert_eq!(compressed.reason, Some("bmp.compression"));
    assert_eq!((compressed.width, compressed.height), (Some(32), Some(32)));

    let sixteen = analyze(
        &bmp_header_only(16, 0),
        StegFormat::Bmp,
        &StegLimits::default(),
    );
    assert_eq!(sixteen.status, StegStatus::Unsupported);
    assert_eq!(sixteen.reason, Some("bmp.bit-depth"));
}

#[test]
fn limits_are_enforced_before_decode() {
    let png = encode_png(
        64,
        64,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &rgb_gradient(64, 64),
    );
    let side_limits = StegLimits {
        max_side: 32,
        ..StegLimits::default()
    };
    let report = analyze(&png, StegFormat::Png, &side_limits);
    assert_eq!(report.status, StegStatus::Oversized);
    assert_eq!(report.reason, Some("limit.side"));
    assert_eq!((report.width, report.height), (Some(64), Some(64)));

    let pixel_limits = StegLimits {
        max_pixels: 100,
        ..StegLimits::default()
    };
    let report = analyze(&png, StegFormat::Png, &pixel_limits);
    assert_eq!(report.status, StegStatus::Oversized);
    assert_eq!(report.reason, Some("limit.pixels"));

    let byte_limits = StegLimits {
        max_input_bytes: 10,
        ..StegLimits::default()
    };
    let report = analyze(&png, StegFormat::Png, &byte_limits);
    assert_eq!(report.status, StegStatus::Oversized);
    assert_eq!(report.reason, Some("limit.bytes"));
    assert_eq!((report.width, report.height), (None, None));
}

#[test]
fn embedded_lsb_noise_raises_the_score() {
    let clean = natural_cover(128, 128);
    let mut embedded = clean.clone();
    embed_lsb(&mut embedded, 50);
    let clean_png = encode_png(
        128,
        128,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &clean,
    );
    let embedded_png = encode_png(
        128,
        128,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &embedded,
    );
    let clean_report = analyze(&clean_png, StegFormat::Png, &StegLimits::default());
    let embedded_report = analyze(&embedded_png, StegFormat::Png, &StegLimits::default());
    assert_eq!(clean_report.status, StegStatus::Completed);
    assert_eq!(embedded_report.status, StegStatus::Completed);
    let clean_score = clean_report
        .global
        .as_ref()
        .and_then(|value| value.score)
        .unwrap_or(0.0);
    let global = embedded_report.global.clone().expect("embedded assessment");
    let embedded_score = global.score.expect("embedded score");
    assert!(
        clean_score < embedded_score,
        "clean {clean_score}, embedded {embedded_score}"
    );
    assert!(embedded_score >= 10.0, "embedded score {embedded_score}");
    assert!(global.methods_used >= 2);
    assert!(matches!(global.band, StegBand::Medium | StegBand::High));
}

#[test]
fn missing_iend_and_damaged_iend_crc_are_corrupt() {
    let png = encode_png(
        64,
        64,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &rgb_gradient(64, 64),
    );
    let mut missing = png.clone();
    missing.truncate(missing.len() - 12);
    assert_eq!(
        analyze(&missing, StegFormat::Png, &StegLimits::default()).status,
        StegStatus::Corrupt
    );
    let mut damaged = png;
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    assert_eq!(
        analyze(&damaged, StegFormat::Png, &StegLimits::default()).status,
        StegStatus::Corrupt
    );
}

#[test]
fn spatial_block_limit_applies_to_both_axes_and_all_planes() {
    let png = encode_png(
        320,
        384,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &rgb_gradient(320, 384),
    );
    for max_blocks in [0, 1, 2, 5, 11, 30] {
        let limits = StegLimits {
            max_blocks,
            max_hotspots: 100,
            ..StegLimits::default()
        };
        let report = analyze(&png, StegFormat::Png, &limits);
        assert_eq!(report.status, StegStatus::Completed);
        assert_eq!(report.blocks_total, 30);
        assert!(report.blocks_scored <= max_blocks);
        let positions = report
            .blocks
            .iter()
            .map(|block| (block.x, block.y))
            .collect::<std::collections::BTreeSet<_>>();
        assert!(positions.len() as u32 <= max_blocks);
        assert!(report.planes.iter().all(|plane| plane.samples == 320 * 384));
    }
}

#[test]
fn magic_mime_mapping_is_exact() {
    assert_eq!(
        StegFormat::from_magic_mime("image/png"),
        Some(StegFormat::Png)
    );
    assert_eq!(
        StegFormat::from_magic_mime("IMAGE/BMP"),
        Some(StegFormat::Bmp)
    );
    assert_eq!(StegFormat::from_magic_mime("image/jpeg"), None);
    assert_eq!(
        StegFormat::from_magic_mime("image/png "),
        Some(StegFormat::Png)
    );
    assert_eq!(StegFormat::from_magic_mime("png"), None);
}
