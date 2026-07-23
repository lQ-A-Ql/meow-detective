//! Diagnostic probe: check whether whitelisted EVTX files on the liuyang
//! sample have stale header chunk counts that cause bounded_clean_evtx_bytes
//! to trim live tail chunks (dropping the newest events).

use app_services::datasource_service::detect_image_filesystem;
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::Read;
use std::path::PathBuf;

const CHANNELS: &[&str] = &[
    "Windows/System32/winevt/Logs/System.evtx",
    "Windows/System32/winevt/Logs/Security.evtx",
    "Windows/System32/winevt/Logs/Application.evtx",
    "Windows/System32/winevt/Logs/Microsoft-Windows-TerminalServices-LocalSessionManager%4Operational.evtx",
];

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn probe_evtx_header_chunk_counts() {
    let fixture = std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LIUYANG_E01_FIXTURE");
    let mut image = E01Reader::open(&fixture).expect("open E01");
    let probe = detect_image_filesystem(&mut image).expect("probe E01");
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("NTFS candidate");
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture).expect("reopen E01"));
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).expect("open NTFS");

    for path in CHANNELS {
        let Ok(mut file) = fs.open_file(path) else {
            eprintln!("{path}: not found");
            continue;
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("read evtx");
        let chunk_count = u16::from_le_bytes(bytes[42..44].try_into().unwrap()) as usize;
        let flags = u32::from_le_bytes(bytes[120..124].try_into().unwrap());
        let declared = 4096 + chunk_count * 65536;
        eprintln!(
            "{path}\n  size={} chunk_count={chunk_count} declared={declared} flags={flags:#x} dirty={} trailing_bytes={}",
            bytes.len(),
            flags & 0x1 != 0,
            bytes.len().saturating_sub(declared),
        );
        let trimmed = bounded_clean(&bytes);
        eprintln!(
            "  parser-visible bytes: full={} trimmed={}",
            bytes.len(),
            trimmed.len()
        );
        let full =
            artifacts_windows::extract_structured_events(&bytes, path).expect("full extraction");
        let old = artifacts_windows::extract_structured_events(trimmed, path)
            .expect("trimmed extraction");
        let full_total =
            full.boot_events.len() + full.security_events.len() + full.application_events.len();
        let old_total =
            old.boot_events.len() + old.security_events.len() + old.application_events.len();
        let mut newest = full
            .boot_events
            .iter()
            .map(|event| event.timestamp.clone())
            .chain(
                full.security_events
                    .iter()
                    .map(|event| event.timestamp.clone()),
            )
            .chain(
                full.application_events
                    .iter()
                    .map(|event| event.timestamp.clone()),
            )
            .collect::<Vec<_>>();
        newest.sort();
        eprintln!(
            "  events: trimmed={old_total} full={full_total} recovered={} newest={:?} warnings(full)={}",
            full_total - old_total,
            newest.last(),
            full.warnings.len()
        );
        for (record_id, event_id, timestamp) in artifacts_windows::probe_newest_records(&bytes, 6) {
            eprintln!("    tail record id={record_id} event={event_id} ts={timestamp}");
        }
        assert!(
            full_total >= old_total,
            "{path}: full parse must never see fewer events than the trimmed parse"
        );
    }
}

/// Mirror of bounded_clean_evtx_bytes in artifacts-windows (kept in sync for
/// this diagnostic only).
fn bounded_clean(bytes: &[u8]) -> &[u8] {
    if bytes.len() < 4096 + 128 || !bytes.starts_with(b"ElfFile\0") {
        return bytes;
    }
    let chunk_count = u16::from_le_bytes(bytes[42..44].try_into().unwrap()) as usize;
    let flags = u32::from_le_bytes(bytes[120..124].try_into().unwrap());
    if flags & 0x1 != 0 || chunk_count == 0 {
        return bytes;
    }
    let declared = 4096usize.saturating_add(chunk_count.saturating_mul(65536));
    if declared > 4096 && declared < bytes.len() {
        &bytes[..declared]
    } else {
        bytes
    }
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn probe_all_channels_newest_records() {
    let fixture = std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LIUYANG_E01_FIXTURE");
    let mut image = E01Reader::open(&fixture).expect("open E01");
    let probe = detect_image_filesystem(&mut image).expect("probe E01");
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("NTFS candidate");
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture).expect("reopen E01"));
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).expect("open NTFS");

    let dir = "Windows/System32/winevt/Logs";
    let mut rows = Vec::new();
    for entry in fs
        .list_children(dir)
        .expect("list winevt logs")
        .into_iter()
        .filter(|entry| !entry.is_dir && entry.name.to_ascii_lowercase().ends_with(".evtx"))
    {
        let path = format!("{dir}/{}", entry.name);
        let Ok(mut file) = fs.open_file(&path) else {
            continue;
        };
        if entry.size > 16 * 1024 * 1024 {
            rows.push((entry.name.clone(), format!("skipped: {} bytes", entry.size)));
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("read evtx");
        let newest = artifacts_windows::probe_newest_records(&bytes, 1);
        match newest.first() {
            Some((record_id, event_id, timestamp)) => rows.push((
                entry.name.clone(),
                format!("record={record_id} event={event_id} ts={timestamp}"),
            )),
            None => rows.push((entry.name.clone(), "no parseable records".to_string())),
        }
    }
    rows.sort_by(|a, b| a.1.cmp(&b.1));
    let mut latest_after_cutoff = 0usize;
    for (name, info) in &rows {
        let after = info.contains("ts=2026-04-20T16:26")
            || info.contains("ts=2026-04-20T17")
            || info.contains("ts=2026-04-20T18")
            || info.contains("ts=2026-04-20T2")
            || info.contains("ts=2026-04-21");
        if after {
            latest_after_cutoff += 1;
        }
        eprintln!(
            "{name}: {info}{}",
            if after { "   <-- AFTER 16:25:33" } else { "" }
        );
    }
    eprintln!(
        "channels={} files_with_records_after_2026-04-20T16:25:33={latest_after_cutoff}",
        rows.len()
    );
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn probe_final_shutdown_sequence_events() {
    let fixture = std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LIUYANG_E01_FIXTURE");
    let mut image = E01Reader::open(&fixture).expect("open E01");
    let probe = detect_image_filesystem(&mut image).expect("probe E01");
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("NTFS candidate");
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture).expect("reopen E01"));
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).expect("open NTFS");
    let mut file = fs
        .open_file("Windows/System32/winevt/Logs/System.evtx")
        .expect("open System.evtx");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("read System.evtx");
    let extraction = artifacts_windows::extract_boot_shutdown_events(
        &bytes,
        "Windows/System32/winevt/Logs/System.evtx",
    )
    .expect("boot extraction");
    let mut events = extraction.events;
    events.sort_by_key(|event| event.record_id.unwrap_or(0));
    for event in events.iter().rev().take(16).rev() {
        eprintln!(
            "  record={:?} id={} kind={} ts={}",
            event.record_id,
            event.event_id,
            event.kind.as_str(),
            event.timestamp
        );
    }
}
