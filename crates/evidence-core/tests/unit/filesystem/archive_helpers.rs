use super::*;
use std::io::{Cursor, Read};

#[test]
fn inflated_limit_accepts_streams_at_the_limit() {
    let mut reader = InflatedLimit::new(Cursor::new(b"abc"), 3);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"abc");
}

#[test]
fn inflated_limit_rejects_streams_past_the_limit() {
    let mut reader = InflatedLimit::new(Cursor::new(b"abcd"), 3);
    let mut buffer = [0u8; 4];
    assert_eq!(reader.read(&mut buffer).unwrap(), 3);
    let error = reader
        .read(&mut buffer)
        .expect_err("oversized stream accepted");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn inflated_limit_accepts_empty_read_buffers() {
    let mut reader = InflatedLimit::new(Cursor::new(b"payload"), 1);
    assert_eq!(reader.read(&mut []).unwrap(), 0);
}
