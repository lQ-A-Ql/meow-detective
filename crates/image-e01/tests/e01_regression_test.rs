use evidence_core::EvidenceReader;
use image_e01::E01Reader;
use std::io::{Read, Seek, SeekFrom};

fn p() -> std::path::PathBuf {
    "E:/pangushi/刘洋/liuyang_pc.E01".into()
}
fn skip() -> bool {
    if !p().exists() {
        eprintln!("SKIP");
        true
    } else {
        false
    }
}

#[test]
fn open_real_file() {
    if skip() {
        return;
    }
    let r = E01Reader::open(&p()).unwrap();
    assert!(r.info().size > 0);
}

#[test]
fn read_first_sector() {
    if skip() {
        return;
    }
    let mut r = E01Reader::open(&p()).unwrap();
    let mut buf = [0u8; 512];
    r.read_exact(&mut buf).unwrap();
    // Verify we got non-zero data (first 4 bytes not all zero)
    let non_zero = buf[0..4].iter().any(|&b| b != 0);
    assert!(non_zero, "first sector is all zeros");
}

#[test]
fn cross_chunk_4k() {
    if skip() {
        return;
    }
    let mut r = E01Reader::open(&p()).unwrap();
    let mut s0 = [0u8; 512];
    r.read_exact(&mut s0).unwrap();
    r.seek(SeekFrom::Start(0)).unwrap();
    let mut cross = [0u8; 4096];
    let n = r.read(&mut cross).unwrap();
    assert_eq!(n, 4096);
    assert_eq!(&cross[0..4], &s0[0..4]);
}

#[test]
fn seek_end_read_last() {
    if skip() {
        return;
    }
    let mut r = E01Reader::open(&p()).unwrap();
    r.seek(SeekFrom::End(-512)).unwrap();
    let mut buf = [0u8; 512];
    r.read_exact(&mut buf).unwrap();
}
