use std::io::Cursor;

use super::*;

#[test]
fn dictionary_lines_strip_bom_crlf_and_blank_values() {
    let mut reader = BufReader::new(Cursor::new(b"\xEF\xBB\xBFsecret\r\n\r\nnext\n"));
    let first = read_bounded_line(&mut reader)
        .expect("first line")
        .expect("value");
    let first = decode_candidate(first).expect("utf8");
    assert_eq!(&*first, "secret");
    let blank = read_bounded_line(&mut reader)
        .expect("blank line")
        .expect("value");
    assert!(decode_candidate(blank).expect("utf8").is_empty());
    let next = read_bounded_line(&mut reader)
        .expect("next line")
        .expect("value");
    assert_eq!(&*decode_candidate(next).expect("utf8"), "next");
}

#[test]
fn dictionary_rejects_invalid_utf8() {
    let mut reader = BufReader::new(Cursor::new(vec![0xff, b'\n']));
    let line = read_bounded_line(&mut reader)
        .expect("line")
        .expect("value");
    assert!(matches!(
        decode_candidate(line),
        Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary contains invalid UTF-8"
        })
    ));
}

#[test]
fn dictionary_rejects_oversized_lines() {
    let input = vec![b'x'; MAX_DICTIONARY_LINE_BYTES + 1];
    let mut reader = BufReader::new(Cursor::new(input));
    assert!(matches!(
        read_bounded_line(&mut reader),
        Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary line exceeds the size limit"
        })
    ));
}

#[test]
fn dictionary_rejects_growth_past_file_limit() {
    assert!(matches!(
        checked_bytes_processed(MAX_DICTIONARY_BYTES, 1),
        Err(BitLockerServiceError::DictionaryInvalid {
            reason: "dictionary file exceeds the size limit"
        })
    ));
}
