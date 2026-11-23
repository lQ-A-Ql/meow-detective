use image_inspection::bmp::{Header, ParseError, RawBmp};

const IMAGE: &[u8] = include_bytes!("../../../vendor/tinybmp-0.7.0/tests/chessboard-8px-24bit.bmp");

#[test]
fn metadata_prefix_and_pixels_share_the_native_header() {
    let full = RawBmp::from_slice(IMAGE).unwrap();
    let header = Header::from_slice(&IMAGE[..full.header().image_data_start]).unwrap();
    assert_eq!(&header, full.header());
    assert_eq!(header.image_data_length(), Some(full.image_data().len()));
}

#[test]
fn invalid_planes_and_overlapping_pixel_data_fail_in_both_entry_points() {
    let mut invalid_planes = IMAGE.to_vec();
    invalid_planes[26..28].copy_from_slice(&0u16.to_le_bytes());
    assert_eq!(
        Header::from_slice(&invalid_planes),
        Err(ParseError::InvalidColorPlanes(0))
    );
    assert_eq!(
        RawBmp::from_slice(&invalid_planes),
        Err(ParseError::InvalidColorPlanes(0))
    );
    let mut overlapping = IMAGE.to_vec();
    overlapping[10..14].copy_from_slice(&40u32.to_le_bytes());
    assert_eq!(
        Header::from_slice(&overlapping),
        Err(ParseError::InvalidImageDataStart)
    );
    assert_eq!(
        RawBmp::from_slice(&overlapping),
        Err(ParseError::InvalidImageDataStart)
    );
}

#[test]
fn truncated_payload_keeps_metadata_readable_and_rejects_pixel_decode() {
    let bytes = &IMAGE[..IMAGE.len() - 1];
    assert!(Header::from_slice(bytes).is_ok());
    assert_eq!(
        RawBmp::from_slice(bytes),
        Err(ParseError::UnexpectedEndOfFile)
    );
}
