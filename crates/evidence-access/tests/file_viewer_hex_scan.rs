use std::sync::atomic::{AtomicBool, Ordering};

use evidence_access::file_service::{scan_chunks, FileServiceError, HexScanProgress};

fn read_window(
    data: &[u8],
    window: usize,
) -> impl FnMut(u64, usize) -> Result<Vec<u8>, FileServiceError> + '_ {
    move |offset, requested| {
        let start = offset as usize;
        if start >= data.len() {
            return Ok(Vec::new());
        }
        let length = requested.min(window).min(data.len() - start);
        Ok(data[start..start + length].to_vec())
    }
}

fn naive_matches(data: &[u8], pattern: &[u8]) -> Vec<u64> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return Vec::new();
    }
    (0..=data.len() - pattern.len())
        .filter(|start| &data[*start..start + pattern.len()] == pattern)
        .map(|start| start as u64)
        .collect()
}

fn collect_progress(
    data: &[u8],
    window: usize,
    pattern: &[u8],
    max_matches: usize,
) -> (
    evidence_access::file_service::HexScanOutcome,
    Vec<HexScanProgress>,
) {
    let mut progress = Vec::new();
    let outcome = scan_chunks(
        data.len() as u64,
        read_window(data, window),
        pattern,
        max_matches,
        &AtomicBool::new(false),
        &mut |update| progress.push(update),
    )
    .expect("scan must succeed");
    (outcome, progress)
}

#[test]
fn finds_overlapping_matches_across_small_chunks_in_order() {
    let data = b"ABABxxABABAA";
    let (outcome, progress) = collect_progress(data, 3, b"AB", 100);

    assert_eq!(outcome.matches, vec![0, 2, 6, 8]);
    assert_eq!(outcome.matches, naive_matches(data, b"AB"));
    assert!(!outcome.truncated);
    assert!(!outcome.cancelled);
    assert_eq!(outcome.scanned_bytes, data.len() as u64);
    assert_eq!(progress.last().unwrap().scanned_bytes, data.len() as u64);
}

#[test]
fn reports_a_match_straddling_the_chunk_boundary_once() {
    let data = b"xxABCDyy";
    let (outcome, _) = collect_progress(data, 3, b"ABCD", 100);

    assert_eq!(outcome.matches, vec![2]);
    assert_eq!(outcome.matches, naive_matches(data, b"ABCD"));
}

#[test]
fn single_byte_pattern_matches_every_occurrence() {
    let data = b"AAA";
    let (outcome, progress) = collect_progress(data, 64, b"A", 100);

    assert_eq!(outcome.matches, vec![0, 1, 2]);
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].scanned_bytes, 3);
}

#[test]
fn handles_files_shorter_than_the_pattern_and_empty_files() {
    let short = b"AB";
    let (outcome, progress) = collect_progress(short, 64, b"ABCDE", 100);
    assert!(outcome.matches.is_empty());
    assert_eq!(outcome.scanned_bytes, 2);
    assert_eq!(progress.len(), 1);

    let empty: &[u8] = b"";
    let mut progress = Vec::new();
    let outcome = scan_chunks(
        0,
        read_window(empty, 64),
        b"AB",
        100,
        &AtomicBool::new(false),
        &mut |update| progress.push(update),
    )
    .unwrap();
    assert!(outcome.matches.is_empty());
    assert_eq!(outcome.scanned_bytes, 0);
    assert_eq!(outcome.total_bytes, 0);
    assert!(progress.is_empty());
}

#[test]
fn stops_at_the_match_cap_and_flags_truncation() {
    let data = b"ABABABAB";
    let (outcome, progress) = collect_progress(data, 64, b"AB", 3);

    assert_eq!(outcome.matches, vec![0, 2, 4]);
    assert!(outcome.truncated);
    assert!(!outcome.cancelled);
    assert!(progress.last().unwrap().truncated);
}

#[test]
fn pre_set_cancel_returns_cancelled_without_reading() {
    let data = b"ABAB";
    let cancel = AtomicBool::new(true);
    let mut progress = Vec::new();
    let outcome = scan_chunks(
        data.len() as u64,
        read_window(data, 64),
        b"AB",
        100,
        &cancel,
        &mut |update| progress.push(update),
    )
    .unwrap();

    assert!(outcome.cancelled);
    assert!(outcome.matches.is_empty());
    assert_eq!(outcome.scanned_bytes, 0);
    assert!(progress.is_empty());
}

#[test]
fn cancel_requested_during_scan_stops_at_the_next_chunk() {
    let data = b"ABxxCD";
    let cancel = AtomicBool::new(false);
    let mut progress = Vec::new();
    let outcome = scan_chunks(
        data.len() as u64,
        read_window(data, 2),
        b"AB",
        100,
        &cancel,
        &mut |update| {
            progress.push(update);
            cancel.store(true, Ordering::Release);
        },
    )
    .unwrap();

    assert!(outcome.cancelled);
    assert_eq!(outcome.scanned_bytes, 2);
    assert_eq!(outcome.matches, vec![0]);
    assert_eq!(progress.len(), 1);
}

#[test]
fn progress_is_monotonic_and_matches_associate_with_reported_offsets() {
    let data = b"0123456789";
    let mut progress = Vec::new();
    let outcome = scan_chunks(
        data.len() as u64,
        read_window(data, 3),
        b"89",
        100,
        &AtomicBool::new(false),
        &mut |update| progress.push(update),
    )
    .unwrap();

    assert_eq!(
        progress
            .iter()
            .map(|update| update.scanned_bytes)
            .collect::<Vec<_>>(),
        vec![3, 6, 9, 10]
    );
    assert_eq!(outcome.matches, vec![8]);
    assert!(progress
        .windows(2)
        .all(|pair| pair[0].scanned_bytes <= pair[1].scanned_bytes));
}

#[test]
fn empty_pattern_is_rejected() {
    let data = b"AB";
    let error = scan_chunks(
        data.len() as u64,
        read_window(data, 64),
        b"",
        100,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(matches!(error, FileServiceError::InvalidInput(_)));
}

#[test]
fn reader_errors_propagate_to_the_caller() {
    let error = scan_chunks(
        4,
        |_offset, _length| {
            Err::<Vec<u8>, _>(FileServiceError::Io(std::io::Error::other(
                "synthetic read failure",
            )))
        },
        b"AB",
        100,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(matches!(error, FileServiceError::Io(_)));
}
