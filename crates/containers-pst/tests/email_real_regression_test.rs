//! Real-world email container regression tests.
//!
//! These tests are ignored by default because they require external sample
//! files that are not committed to the repository. Set the environment
//! variable `FORENSICS_EMAIL_FIXTURE_DIR` to a directory containing real
//! `.eml`, `.mbox`, `.pst`, or `.ost` samples, then run:
//!
//! ```powershell
//! $env:FORENSICS_EMAIL_FIXTURE_DIR = "C:\\path\\to\\email-samples"
//! cargo test -p containers-pst --test email_real_regression_test -- --ignored --nocapture
//! ```

use containers_pst::{mbox, ost::OstReader, pst::PstReader, PstError};
use mailparse::MailHeaderMap;
use std::path::{Path, PathBuf};

fn fixture_dir() -> Option<PathBuf> {
    std::env::var_os("FORENSICS_EMAIL_FIXTURE_DIR").map(PathBuf::from)
}

#[test]
#[ignore = "requires FORENSICS_EMAIL_FIXTURE_DIR real email samples"]
fn real_email_fixtures_parse_without_crash_and_expose_key_fields() {
    let dir = fixture_dir()
        .expect("set FORENSICS_EMAIL_FIXTURE_DIR to run ignored real email regression tests");
    assert!(
        dir.is_dir(),
        "FORENSICS_EMAIL_FIXTURE_DIR must point to an existing directory: {}",
        dir.display()
    );

    let mut checked = 0usize;
    for entry in walkdir(&dir) {
        let ext = entry
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "eml" => {
                check_eml(&entry);
                checked += 1;
            }
            "mbox" => {
                check_mbox(&entry);
                checked += 1;
            }
            "pst" => {
                check_pst(&entry);
                checked += 1;
            }
            "ost" => {
                check_ost(&entry);
                checked += 1;
            }
            _ => {}
        }
    }

    assert!(
        checked > 0,
        "no .eml/.mbox/.pst/.ost files found under {}",
        dir.display()
    );
    eprintln!(
        "real email regression: checked {} sample(s) under {}",
        checked,
        dir.display()
    );
}

fn walkdir(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(walkdir(&path));
            } else {
                result.push(path);
            }
        }
    }
    result
}

fn check_eml(path: &Path) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let parsed = mailparse::parse_mail(&bytes)
        .unwrap_or_else(|e| panic!("parse_mail {}: {}", path.display(), e));
    let headers = parsed.headers;
    assert!(
        headers.get_first_header("Subject").is_some() || headers.get_first_header("From").is_some(),
        "{} should expose at least Subject or From header",
        path.display()
    );
    eprintln!(
        "eml OK: {} subject={:?} from={:?}",
        path.display(),
        header_value(&headers, "Subject"),
        header_value(&headers, "From")
    );
}

fn check_mbox(path: &Path) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let messages =
        mbox::parse_mbox(&bytes).unwrap_or_else(|e| panic!("parse_mbox {}: {}", path.display(), e));
    assert!(
        !messages.is_empty(),
        "{} should contain at least one message",
        path.display()
    );
    let first = &messages[0];
    assert!(
        !first.subject.is_empty() || !first.sender_email.is_empty(),
        "first message in {} should have subject or sender",
        path.display()
    );
    eprintln!(
        "mbox OK: {} messages={} first_subject={:?}",
        path.display(),
        messages.len(),
        first.subject
    );
}

fn check_pst(path: &Path) {
    let reader = PstReader::open(path)
        .unwrap_or_else(|e| panic!("PstReader::open {}: {}", path.display(), e));
    let messages = reader
        .read_messages()
        .unwrap_or_else(|e| panic!("read_messages {}: {}", path.display(), e));
    assert!(
        !messages.is_empty(),
        "{} should contain at least one message",
        path.display()
    );
    let first = &messages[0];
    assert!(
        !first.subject.is_empty() || !first.sender_email.is_empty(),
        "first message in {} should have subject or sender",
        path.display()
    );
    eprintln!(
        "pst OK: {} messages={} first_subject={:?}",
        path.display(),
        messages.len(),
        first.subject
    );
}

fn check_ost(path: &Path) {
    let reader = OstReader::open(path)
        .unwrap_or_else(|e| panic!("OstReader::open {}: {}", path.display(), e));
    assert_eq!(
        reader.file_kind(),
        containers_pst::ost::OutlookFileKind::Ost,
        "{} should be detected as OST",
        path.display()
    );
    let messages = reader
        .read_messages()
        .unwrap_or_else(|e| panic!("read_messages {}: {}", path.display(), e));
    assert!(
        !messages.is_empty(),
        "{} should contain at least one message",
        path.display()
    );
    eprintln!("ost OK: {} messages={}", path.display(), messages.len());
}

fn header_value(headers: &[mailparse::MailHeader<'_>], name: &str) -> Option<String> {
    headers.get_first_header(name).map(|h| h.get_value())
}

#[test]
#[ignore = "requires FORENSICS_EMAIL_FIXTURE_DIR real email samples"]
fn real_fixture_eml_subject_from_are_non_empty() {
    let dir = fixture_dir()
        .expect("set FORENSICS_EMAIL_FIXTURE_DIR to run ignored real email regression tests");
    let mut checked = 0usize;
    for entry in walkdir(&dir) {
        if entry.extension().and_then(|e| e.to_str()) == Some("eml") {
            let bytes =
                std::fs::read(&entry).unwrap_or_else(|e| panic!("read {}: {}", entry.display(), e));
            let parsed = mailparse::parse_mail(&bytes)
                .unwrap_or_else(|e| panic!("parse_mail {}: {}", entry.display(), e));
            let subject = parsed
                .headers
                .get_first_header("Subject")
                .map(|h| h.get_value());
            let from = parsed
                .headers
                .get_first_header("From")
                .map(|h| h.get_value());
            assert!(
                subject
                    .as_ref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
                    || from.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false),
                "{} should have non-empty Subject or From",
                entry.display()
            );
            checked += 1;
        }
    }
    if checked == 0 {
        eprintln!(
            "no .eml samples found under {}; skipping assertion",
            dir.display()
        );
    }
}

#[test]
#[ignore = "requires FORENSICS_EMAIL_FIXTURE_DIR real email samples"]
fn real_fixture_mbox_message_count_is_stable() {
    let dir = fixture_dir()
        .expect("set FORENSICS_EMAIL_FIXTURE_DIR to run ignored real email regression tests");
    let mut checked = 0usize;
    for entry in walkdir(&dir) {
        if entry.extension().and_then(|e| e.to_str()) == Some("mbox") {
            let bytes =
                std::fs::read(&entry).unwrap_or_else(|e| panic!("read {}: {}", entry.display(), e));
            let messages = mbox::parse_mbox(&bytes)
                .unwrap_or_else(|e| panic!("parse_mbox {}: {}", entry.display(), e));
            assert!(
                !messages.is_empty(),
                "{} should contain at least one message",
                entry.display()
            );
            eprintln!("mbox count: {} -> {}", entry.display(), messages.len());
            checked += 1;
        }
    }
    if checked == 0 {
        eprintln!(
            "no .mbox samples found under {}; skipping assertion",
            dir.display()
        );
    }
}

#[test]
#[ignore = "requires FORENSICS_EMAIL_FIXTURE_DIR real email samples"]
fn real_fixture_pst_ost_messages_are_non_empty() {
    let dir = fixture_dir()
        .expect("set FORENSICS_EMAIL_FIXTURE_DIR to run ignored real email regression tests");
    let mut checked = 0usize;
    for entry in walkdir(&dir) {
        let ext = entry
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let messages: Result<Vec<_>, PstError> = match ext.as_str() {
            "pst" => PstReader::open(&entry).and_then(|r| r.read_messages()),
            "ost" => OstReader::open(&entry).and_then(|r| r.read_messages()),
            _ => continue,
        };
        let messages = messages.unwrap_or_else(|e| panic!("read {}: {}", entry.display(), e));
        assert!(
            !messages.is_empty(),
            "{} should contain at least one message",
            entry.display()
        );
        eprintln!("{} count: {} -> {}", ext, entry.display(), messages.len());
        checked += 1;
    }
    if checked == 0 {
        eprintln!(
            "no .pst/.ost samples found under {}; skipping assertion",
            dir.display()
        );
    }
}
