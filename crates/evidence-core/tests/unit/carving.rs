use super::*;
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Write},
    sync::atomic::{AtomicBool, Ordering},
};

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(data);
    bytes.extend_from_slice(&crc32fast::hash(&bytes[4..]).to_be_bytes());
    bytes
}
fn png() -> Vec<u8> {
    let mut ihdr = vec![0; 13];
    ihdr[..4].copy_from_slice(&1u32.to_be_bytes());
    ihdr[4..8].copy_from_slice(&1u32.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 6;
    let mut compressor =
        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    compressor.write_all(&[0, 255, 0, 0, 255]).unwrap();
    [
        PNG_SIGNATURE.to_vec(),
        chunk(b"IHDR", &ihdr),
        chunk(b"IDAT", &compressor.finish().unwrap()),
        chunk(b"IEND", &[]),
    ]
    .concat()
}
fn scope(bytes: &[u8]) -> CarveScope {
    CarveScope {
        source_id: "source".into(),
        revision: "revision".into(),
        view: "logicalDisk".into(),
        source_length: bytes.len() as u64,
        offset: 0,
        length: bytes.len() as u64,
    }
}
fn run(bytes: Vec<u8>, window: usize) -> (ScanReport, Vec<CarveCandidate>) {
    let source = scope(&bytes);
    let mut output = Vec::new();
    let report = scan_png(
        &mut Cursor::new(bytes),
        ScanRequest {
            scope: &source,
            limits: CarveLimits {
                window_bytes: window,
                ..Default::default()
            },
            cancel: &AtomicBool::new(false),
            progress: &|_, _| {},
        },
        |c| {
            output.push(c);
            Ok(())
        },
    )
    .unwrap();
    (report, output)
}

#[test]
fn cross_window_signature_is_emitted_once_with_exact_scope_length_and_hash() {
    let image = png();
    let mut bytes = vec![0; 13];
    bytes.extend_from_slice(&image);
    bytes.extend_from_slice(&[0; 31]);
    let (report, results) = run(bytes, 16);
    assert_eq!((report.candidates, report.complete), (1, 1));
    let candidate = &results[0];
    assert_eq!(candidate.offset, 13);
    assert_eq!(candidate.length, Some(image.len() as u64));
    assert_eq!(
        candidate.sha256,
        Some(format!("{:x}", Sha256::digest(&image)))
    );
    assert_eq!(candidate.source_id, "source");
    assert_eq!(candidate.revision, "revision");
    assert_eq!(candidate.view, "logicalDisk");
    assert_eq!(candidate.validator_version, PNG_VALIDATOR_VERSION);
}
#[test]
fn bad_crc_truncation_false_magic_and_unsupported_layout_are_not_complete() {
    let mut bad_crc = png();
    let end = bad_crc.len() - 1;
    bad_crc[end] ^= 1;
    assert_eq!(run(bad_crc, 64).1[0].reason, "crcMismatch");
    let mut truncated = png();
    truncated.truncate(truncated.len() - 5);
    assert_eq!(run(truncated, 64).1[0].status, CarveStatus::Partial);
    let fake = [PNG_SIGNATURE.as_slice(), &[0; 32]].concat();
    assert_eq!(run(fake, 16).0.complete, 0);
    let mut unsupported = png();
    unsupported[24] = 16;
    assert_eq!(run(unsupported, 64).1[0].reason, "unsupportedImageLayout");
}
#[test]
fn chunk_limits_reversed_ranges_and_u64_overflow_are_bounded() {
    let image = png();
    let mut source = scope(&image);
    source.offset = u64::MAX;
    source.length = 2;
    assert!(matches!(source.end(), Err(CarveError::InvalidRange)));
    let mut huge = PNG_SIGNATURE.to_vec();
    huge.extend_from_slice(&u32::MAX.to_be_bytes());
    huge.extend_from_slice(b"IHDR");
    assert_eq!(run(huge, 16).1[0].reason, "chunkLimit");
    let source = scope(&image);
    let result = scan_png(
        &mut Cursor::new(image),
        ScanRequest {
            scope: &source,
            limits: CarveLimits {
                validation_bytes: 0,
                ..Default::default()
            },
            cancel: &AtomicBool::new(false),
            progress: &|_, _| {},
        },
        |_| Ok(()),
    );
    assert!(matches!(result, Err(CarveError::Limit)));
}
#[test]
fn candidate_limit_consumer_error_and_cancellation_stop_the_scan() {
    let bytes = [png(), png()].concat();
    let source = scope(&bytes);
    let mut count = 0;
    let result = scan_png(
        &mut Cursor::new(&bytes),
        ScanRequest {
            scope: &source,
            limits: CarveLimits {
                max_candidates: 1,
                ..Default::default()
            },
            cancel: &AtomicBool::new(false),
            progress: &|_, _| {},
        },
        |_| {
            count += 1;
            Ok(())
        },
    );
    assert!(matches!(result, Err(CarveError::Limit)));
    assert_eq!(count, 1);
    let result = scan_png(
        &mut Cursor::new(&bytes),
        ScanRequest {
            scope: &source,
            limits: Default::default(),
            cancel: &AtomicBool::new(false),
            progress: &|_, _| {},
        },
        |_| Err(CarveError::ConsumerStopped),
    );
    assert!(matches!(result, Err(CarveError::ConsumerStopped)));
    let cancel = AtomicBool::new(false);
    let progress = |_, _| cancel.store(true, Ordering::Release);
    let result = scan_png(
        &mut Cursor::new(&bytes),
        ScanRequest {
            scope: &source,
            limits: CarveLimits {
                window_bytes: 16,
                ..Default::default()
            },
            cancel: &cancel,
            progress: &progress,
        },
        |_| Ok(()),
    );
    assert!(matches!(result, Err(CarveError::Cancelled)));
}
#[test]
fn bounded_mutation_smoke_png_never_accepts_an_unverified_digest() {
    let original = png();
    for seed in 0..256usize {
        let mut bytes = original.clone();
        let index = (seed * 37) % bytes.len();
        bytes[index] ^= (seed as u8) | 1;
        let (_, results) = run(bytes.clone(), 16);
        for result in results
            .into_iter()
            .filter(|r| r.status == CarveStatus::Complete)
        {
            let length = result.length.unwrap() as usize;
            let offset = result.offset as usize;
            assert_eq!(
                result.sha256,
                Some(format!(
                    "{:x}",
                    Sha256::digest(&bytes[offset..offset + length])
                ))
            );
        }
    }
}
