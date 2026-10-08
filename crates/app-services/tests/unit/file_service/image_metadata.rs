#[test]
fn parses_little_endian_gps_coordinates_and_altitude() {
    let mut bytes = vec![0u8; 256];
    bytes[0..2].copy_from_slice(b"II");
    bytes[2..4].copy_from_slice(&42u16.to_le_bytes());
    bytes[4..8].copy_from_slice(&8u32.to_le_bytes());
    bytes[8..10].copy_from_slice(&1u16.to_le_bytes());
    // GPSInfo pointer.
    bytes[10..12].copy_from_slice(&0x8825u16.to_le_bytes());
    bytes[12..14].copy_from_slice(&4u16.to_le_bytes());
    bytes[14..18].copy_from_slice(&1u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&32u32.to_le_bytes());
    bytes[32..34].copy_from_slice(&4u16.to_le_bytes());
    let entries = [
        (1u16, 2u16, 2u32, b"N\0\0\0".to_vec()),
        (2u16, 5u16, 3u32, 128u32.to_le_bytes().to_vec()),
        (3u16, 2u16, 2u32, b"E\0\0\0".to_vec()),
        (4u16, 5u16, 3u32, 152u32.to_le_bytes().to_vec()),
    ];
    for (index, (tag, typ, count, value)) in entries.into_iter().enumerate() {
        let offset = 34 + index * 12;
        bytes[offset..offset + 2].copy_from_slice(&tag.to_le_bytes());
        bytes[offset + 2..offset + 4].copy_from_slice(&typ.to_le_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&count.to_le_bytes());
        bytes[offset + 8..offset + 12].copy_from_slice(&value);
    }
    // 1° 30' 0", 2° 0' 0".
    for (offset, numerator) in [(128, 1), (136, 30), (144, 0), (152, 2), (160, 0), (168, 0)] {
        bytes[offset..offset + 4].copy_from_slice(&(numerator as u32).to_le_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&1u32.to_le_bytes());
    }
    let mut metadata = super::ImageMetadataDto {
        status: super::ImageMetadataStatusDto::Absent,
        format: None,
        width: None,
        height: None,
        orientation: None,
        make: None,
        model: None,
        software: None,
        date_time_original: None,
        create_date: None,
        modify_date: None,
        lens_model: None,
        latitude: None,
        longitude: None,
        altitude: None,
        gps_date_time: None,
    };
    super::parse_tiff(&bytes, &mut metadata);
    assert_eq!(metadata.latitude, Some(1.5));
    assert_eq!(metadata.longitude, Some(2.0));
}

#[test]
fn rejects_truncated_tiff_and_jpeg_segments_without_panicking() {
    let mut metadata = super::ImageMetadataDto {
        status: super::ImageMetadataStatusDto::Absent,
        format: None,
        width: None,
        height: None,
        orientation: None,
        make: None,
        model: None,
        software: None,
        date_time_original: None,
        create_date: None,
        modify_date: None,
        lens_model: None,
        latitude: None,
        longitude: None,
        altitude: None,
        gps_date_time: None,
    };
    super::parse_tiff(b"II*\0\xff", &mut metadata);
    assert!(super::jpeg_dimensions(&[0xff, 0xd8, 0xff, 0xe1, 0, 1]).is_none());
}
