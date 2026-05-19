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

// --- Phase 18: multi-segment detection + regression ---

#[test]
fn multi_segment_with_only_first_file_works() {
    // Verify that when only .E01 exists (no .E02+), single-segment
    // mode still works correctly — the segment detection logic
    // assigns all chunks to segment 0.
    use std::io::Write;

    let dir = std::env::temp_dir().join("e01_single_seg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let e01_path = dir.join("test.E01");

    let chunk_sectors: u32 = 8;
    let sectors: u64 = 16;
    let chunk_bytes = (chunk_sectors * 512) as usize;

    let mut f = std::fs::File::create(&e01_path).unwrap();
    f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00").unwrap();

    // volume section: desc at 13, content at 89, next desc at 89+36=125
    let mut vol = vec![0u8; 36];
    vol[12..16].copy_from_slice(&chunk_sectors.to_le_bytes());
    vol[16..24].copy_from_slice(&sectors.to_le_bytes());
    f.write_all(&sdesc("volume", 125, 36)).unwrap();
    f.write_all(&vol).unwrap();

    // table (1 chunk): desc at 125, content 16 bytes, next desc at 125+76+16=217
    // Content format: [0..8]=0, [8..16]=table_base, [12..16]=entry0 (overlaps base[4..8])
    let mut tbl = vec![0u8; 16];
    // data starts after done desc: 217 + 76 = 293
    let data_start: u64 = 217 + 76;
    tbl[8..16].copy_from_slice(&data_start.to_le_bytes());
    // entry 0 at offset 12 = 0 (already zeroed)
    f.write_all(&sdesc("table", 217, tbl.len() as u64)).unwrap();
    f.write_all(&tbl).unwrap();

    // done: desc at 217, next=0 (end of section chain)
    f.write_all(&sdesc("done", 0, 0)).unwrap();

    // chunk 0 at offset 293+: raw data after all sections
    let mut c0 = vec![0u8; chunk_bytes];
    c0[..5].copy_from_slice(b"HELLO");
    f.write_all(&c0).unwrap();
    f.flush().unwrap();
    drop(f);

    let mut r = E01Reader::open(&e01_path).unwrap();
    assert_eq!(r.info().size, sectors * 512);
    let mut buf = [0u8; 5];
    r.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"HELLO");

    let _ = std::fs::remove_dir_all(&dir);
}

fn sdesc(stype: &str, next: u64, size: u64) -> [u8; 76] {
    let mut d = [0u8; 76];
    let b = stype.as_bytes();
    let n = b.len().min(16);
    d[0..n].copy_from_slice(&b[..n]);
    d[16..24].copy_from_slice(&next.to_le_bytes());
    d[24..32].copy_from_slice(&size.to_le_bytes());
    d
}
