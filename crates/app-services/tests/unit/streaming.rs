use super::*;
use std::io::Cursor;

#[test]
fn test_streaming_hasher() {
    let data = b"Hello, World!";
    let mut hasher = StreamingHasher::new();
    hasher.process_chunk(data).unwrap();
    let result = hasher.finalize().unwrap();
    assert_eq!(result.bytes_processed, 13);
}

#[test]
fn test_streaming_reader() {
    let data = vec![0u8; 1000];
    let mut reader = StreamingReader::new(Cursor::new(&data), 1000, 64);

    let mut total_read = 0;
    while !reader.is_eof() {
        let chunk = reader.read_chunk().unwrap();
        total_read += chunk.len();
        if chunk.is_empty() {
            break;
        }
    }

    assert_eq!(total_read, 1000);
    assert_eq!(reader.progress(), 100);
}

#[test]
fn test_process_file_streaming() {
    let data = b"test data for streaming";
    let mut cursor = Cursor::new(data);
    let mut hasher = StreamingHasher::new();

    let results = process_file_streaming(&mut cursor, &mut [&mut hasher]).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].bytes_processed, data.len() as u64);
}
