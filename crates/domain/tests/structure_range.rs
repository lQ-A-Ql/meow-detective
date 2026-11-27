use domain::structure::{ByteRange, ByteRangeError};

#[test]
fn range_rejects_address_overflow_and_target_overrun() {
    assert_eq!(ByteRange::new(u64::MAX, 1), Err(ByteRangeError::Overflow));
    let range = ByteRange::new(4093, 8177).unwrap();
    assert_eq!(range.start(), 4093);
    assert_eq!(range.length(), 8177);
    assert_eq!(range.end(), 12270);
    assert_eq!(
        range.validate(12269),
        Err(ByteRangeError::OutOfBounds {
            logical_size: 12269
        })
    );
    range.validate(12270).unwrap();
}

#[test]
fn empty_range_is_valid_only_inside_target_address_space() {
    ByteRange::new(0, 0).unwrap().validate(0).unwrap();
    ByteRange::new(32, 0).unwrap().validate(32).unwrap();
    assert!(ByteRange::new(33, 0).unwrap().validate(32).is_err());
    ByteRange::new(u64::MAX, 0)
        .unwrap()
        .validate(u64::MAX)
        .unwrap();
}
