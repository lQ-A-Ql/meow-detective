#[path = "support/wof_file.rs"]
mod file;
#[path = "support/wof_frames.rs"]
mod frames;
#[path = "support/wof_image.rs"]
mod image;

use evidence_core::FileSystemReader;
use std::io::{Read, Seek, SeekFrom};

#[test]
fn all_algorithms_recover_cross_chunk_ranges_buffered_content_and_seekable_streams() {
    for (algorithm, chunk_size) in [(0, 4096), (1, 32768), (2, 8192), (3, 16384)] {
        let (compressed, first) = if algorithm == 1 {
            frames::lzx(chunk_size, true, b"MZprefix")
        } else {
            (frames::xpress(chunk_size as u16), vec![b'A'; chunk_size])
        };
        let tail = b"tail-data";
        let mut ads = (compressed.len() as u32).to_le_bytes().to_vec();
        ads.extend_from_slice(&compressed);
        ads.extend_from_slice(tail);
        let mut expected = first;
        expected.extend_from_slice(tail);
        let (fs, _temporary) =
            file::filesystem(image::bytes(algorithm, expected.len() as u64, &ads));
        assert_eq!(
            fs.file_size_by_inode(6).unwrap(),
            Some(expected.len() as u64)
        );
        assert_eq!(
            fs.read_file_range_by_inode(6, chunk_size as u64 - 5, 14)
                .unwrap(),
            expected[chunk_size - 5..]
        );
        assert!(fs
            .read_file_range_by_inode(6, expected.len() as u64, 8)
            .unwrap()
            .is_empty());
        let mut buffered = Vec::new();
        fs.open_file("mft:6")
            .unwrap()
            .read_to_end(&mut buffered)
            .unwrap();
        assert_eq!(buffered, expected);
        assert!(fs.supports_file_stream_by_inode(6).unwrap());
        let mut stream = fs.into_file_stream_by_inode(6).unwrap();
        stream.seek(SeekFrom::End(-9)).unwrap();
        let mut value = [0; 9];
        stream.read_exact(&mut value).unwrap();
        assert_eq!(&value, tail);
        stream.seek(SeekFrom::Start(0)).unwrap();
        let mut full = Vec::new();
        stream.read_to_end(&mut full).unwrap();
        assert_eq!(full, expected);
    }
}

#[test]
fn malformed_tables_missing_ads_and_unknown_algorithms_fail_closed() {
    let frame = frames::xpress(4096);
    for offset in [0, u32::MAX] {
        let mut ads = offset.to_le_bytes().to_vec();
        ads.extend_from_slice(&frame);
        ads.extend_from_slice(b"x");
        let (fs, _temporary) = file::filesystem(image::bytes(0, 4097, &ads));
        assert!(fs.read_file_range_by_inode(6, 0, 1).is_err());
    }
    let (fs, _temporary) = file::filesystem(image::bytes(9, 4096, &frame));
    assert_eq!(
        fs.read_file_range_by_inode(6, 0, 1).unwrap_err().kind(),
        std::io::ErrorKind::Unsupported
    );
    let (fs, _temporary) = file::filesystem(image::bytes(0, 4096, &[0; 2]));
    assert!(fs.read_file_range_by_inode(6, 0, 1).is_err());
    let mut missing = image::bytes(0, 4096, &frame);
    missing[7168 + 128..7168 + 132].copy_from_slice(&0x90u32.to_le_bytes());
    let (fs, _temporary) = file::filesystem(missing);
    assert!(fs.read_file_range_by_inode(6, 0, 1).is_err());
}
