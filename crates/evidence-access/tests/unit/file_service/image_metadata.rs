#[path = "image_metadata_fixture.rs"]
mod fixture;

fn metadata(bytes: &[u8]) -> super::ImageMetadataDto {
    super::metadata_from_reader(bytes.len() as u64, |offset, length| {
        Ok(bytes[offset as usize..offset as usize + length as usize].to_vec())
    })
    .unwrap()
}

#[test]
fn parses_child_ifds_and_utc_gps_in_both_byte_orders_and_all_containers() {
    for little in [true, false] {
        let tiff = fixture::tiff(little);
        for bytes in [
            tiff.clone(),
            fixture::jpeg(&tiff),
            fixture::png(Some(&tiff)),
            fixture::webp(&tiff),
        ] {
            let parsed = metadata(&bytes);
            assert_eq!(parsed.status, super::ImageMetadataStatusDto::Present);
            assert_eq!(parsed.orientation, Some(6));
            assert_eq!(
                parsed.date_time_original.as_deref(),
                Some("2026:10:14 12:34:56")
            );
            assert_eq!(parsed.create_date.as_deref(), Some("2026:10:14 12:34:57"));
            assert_eq!(parsed.lens_model.as_deref(), Some("Lens 50"));
            assert_eq!(parsed.latitude, Some(-1.5));
            assert_eq!(parsed.longitude, Some(-2.0));
            assert_eq!(parsed.altitude, Some(-10.0));
            assert_eq!(
                parsed.gps_date_time.as_deref(),
                Some("2026-10-14T12:34:56.500Z")
            );
        }
    }
}

#[test]
fn malformed_ifds_and_gps_fail_without_exposing_partial_metadata() {
    let mut inputs = Vec::new();
    let mut bad = fixture::tiff(true);
    bad[8..10].copy_from_slice(&100u16.to_le_bytes());
    inputs.push(bad);
    let mut bad = fixture::tiff(true);
    bad[30..34].copy_from_slice(&u32::MAX.to_le_bytes());
    inputs.push(bad);
    let mut bad = fixture::tiff(true);
    bad[42..46].copy_from_slice(&8u32.to_le_bytes());
    inputs.push(bad);
    let mut bad = fixture::tiff(true);
    bad[388..392].copy_from_slice(&0u32.to_le_bytes());
    inputs.push(bad);
    let mut bad = fixture::tiff(true);
    bad[440..444].copy_from_slice(&25u32.to_le_bytes());
    inputs.push(bad);
    inputs.push(b"II*\0\xff".to_vec());
    inputs.push(vec![0xff, 0xd8, 0xff, 0xe1, 0, 1]);
    for bytes in inputs {
        let parsed = metadata(&bytes);
        assert_eq!(
            parsed.status,
            super::ImageMetadataStatusDto::Corrupt,
            "{bytes:x?}"
        );
        assert!(parsed.latitude.is_none());
        assert!(parsed.date_time_original.is_none());
    }
}

#[test]
fn empty_exif_is_distinct_from_invalid_container_and_unsupported_bytes() {
    let parsed = metadata(&fixture::png(None));
    assert_eq!(parsed.status, super::ImageMetadataStatusDto::Absent);
    assert_eq!((parsed.width, parsed.height), (Some(2), Some(3)));
    assert_eq!(
        metadata(b"plain text").status,
        super::ImageMetadataStatusDto::Unsupported
    );
    let mut png = fixture::png(Some(&fixture::tiff(true)));
    png[41] ^= 1;
    assert_eq!(
        metadata(&png).status,
        super::ImageMetadataStatusDto::Corrupt
    );
}

#[test]
fn bounded_metadata_can_follow_chunks_beyond_the_prefix_without_reading_pixels() {
    let tiff = fixture::tiff(true);
    let mut bytes = fixture::png(None);
    bytes.truncate(bytes.len() - 12);
    fixture::png_chunk(&mut bytes, b"IDAT", &vec![0; 4 * 1024 * 1024]);
    fixture::png_chunk(&mut bytes, b"eXIf", &tiff);
    fixture::png_chunk(&mut bytes, b"IEND", &[]);
    let mut total = 0;
    let parsed = super::metadata_from_reader(bytes.len() as u64, |offset, length| {
        total += length as usize;
        Ok(bytes[offset as usize..offset as usize + length as usize].to_vec())
    })
    .unwrap();
    assert_eq!(parsed.status, super::ImageMetadataStatusDto::Present);
    assert!(total < 1024);
}

#[test]
fn oversized_exif_and_source_io_errors_remain_explicit() {
    let bytes = fixture::png(Some(&vec![0; 2 * 1024 * 1024]));
    assert_eq!(
        metadata(&bytes).status,
        super::ImageMetadataStatusDto::Oversized
    );
    assert!(matches!(
        super::metadata_from_reader(16, |_, _| Err(super::FileServiceError::Security(
            "denied".into()
        ))),
        Err(super::FileServiceError::Security(_))
    ));
}
