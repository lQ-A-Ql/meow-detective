//! Throughput sanity tests for email container parsing.
//!
//! These tests run in the normal `cargo test` suite and guard against
//! egregious regressions in mbox / PST parsing speed. They are not a
//! replacement for a full Criterion benchmark suite, but they do enforce
//! the V2 acceptance thresholds on medium-sized synthetic inputs.

use containers_pst::{mbox, pst::PstReader};
use std::time::Instant;

/// V2 acceptance: a 1 MiB mbox must parse in under 1 second on dev hardware.
#[test]
fn mbox_1mb_parses_under_one_second() {
    let data = build_mbox_bytes(1_024 * 1_024);
    let start = Instant::now();
    let messages = mbox::parse_mbox(&data).expect("mbox parse should succeed");
    let elapsed = start.elapsed();

    assert!(
        !messages.is_empty(),
        "1 MiB mbox should contain at least one message"
    );
    assert!(
        elapsed.as_secs_f64() < 1.0,
        "1 MiB mbox parse took {:.3}s, expected < 1.0s",
        elapsed.as_secs_f64()
    );

    let mb = data.len() as f64 / (1_024.0 * 1_024.0);
    let throughput = mb / elapsed.as_secs_f64().max(1e-9);
    eprintln!(
        "mbox throughput: {:.2} MiB parsed in {:.3}s -> {:.2} MiB/s ({} messages)",
        mb,
        elapsed.as_secs_f64(),
        throughput,
        messages.len()
    );
}

/// Synthetic PST fixtures are small; this test establishes a baseline so that
/// future streaming work has a regression anchor.
#[test]
fn synthetic_pst_10_messages_parses_under_100ms() {
    let tmp = tempfile::NamedTempFile::with_suffix(".pst").unwrap();
    let data = containers_pst::pst::build_synthetic_pst_with_messages(10);
    std::fs::write(tmp.path(), &data).unwrap();

    let start = Instant::now();
    let reader = PstReader::open(tmp.path()).expect("open synthetic PST");
    let messages = reader.read_messages().expect("read synthetic PST messages");
    let elapsed = start.elapsed();

    assert_eq!(messages.len(), 10);
    assert!(
        elapsed.as_secs_f64() < 0.1,
        "10-message synthetic PST parse took {:.3}s, expected < 0.1s",
        elapsed.as_secs_f64()
    );

    eprintln!(
        "synthetic pst throughput: {} messages in {:.3}ms",
        messages.len(),
        elapsed.as_secs_f64() * 1_000.0
    );
}

fn build_mbox_bytes(target_size: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(target_size);
    let separator = "From sender@example.com Mon Jun 16 10:00:00 2025\r\n";
    let message = b"From: Sender <sender@example.com>\r\n\
        To: Recipient <recipient@example.com>\r\n\
        Subject: Synthetic throughput message number {idx}\r\n\
        Date: Mon, 16 Jun 2025 10:00:00 +0000\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\
        \r\n\
        This is the body of synthetic message number {idx}. It is long enough to \
        contribute to the overall file size without making the header logic dominant. \
        Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor \
        incididunt ut labore et dolore magna aliqua.\r\n";

    let mut idx = 0usize;
    while out.len() < target_size {
        out.extend_from_slice(separator.as_bytes());
        let body = std::str::from_utf8(message)
            .unwrap()
            .replace("{idx}", &idx.to_string());
        out.extend_from_slice(body.as_bytes());
        out.extend_from_slice(b"\r\n");
        idx += 1;
    }
    out
}
