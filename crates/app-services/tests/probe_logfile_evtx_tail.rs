//! Diagnostic probe: hunt the "missing" 2026-04-20T16:25:35 Kernel-General 13
//! shutdown event inside the NTFS $LogFile of the liuyang E01. If the VM
//! shows the event but the static System.evtx does not, the record should be
//! present in the journal (flushed to $LogFile but not to the evtx data area).

use app_services::datasource_service::detect_image_filesystem;
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::Read;
use std::path::PathBuf;

const FILETIME_EPOCH_DIFF_100NS: u64 = 116_444_736_000_000_000;

fn filetime_utc(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> u64 {
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + u64::from(hour) * 3600 + u64::from(minute) * 60 + u64::from(second);
    FILETIME_EPOCH_DIFF_100NS + secs * 10_000_000
}

fn days_from_civil(year: i32, month: u32, day: u32) -> u64 {
    let y = if month <= 2 { year - 1 } else { year } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as u64
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn probe_logfile_for_unflushed_evtx_tail() {
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

    let mut file = fs.open_file("$LogFile").expect("open $LogFile");
    let mut logfile = Vec::new();
    file.read_to_end(&mut logfile).expect("read $LogFile");
    eprintln!("$LogFile size: {}", logfile.len());

    // FILETIME window around the shutdown tail: 16:25:30 .. 16:26:30 UTC.
    let lo = filetime_utc(2026, 4, 20, 16, 25, 30);
    let hi = filetime_utc(2026, 4, 20, 16, 26, 30);
    eprintln!("FILETIME window: {lo:#x} .. {hi:#x}");

    let mut window_hits = 0usize;
    let mut samples = Vec::new();
    for (offset, chunk) in logfile.windows(8).enumerate() {
        let value = u64::from_le_bytes(chunk.try_into().unwrap());
        if (lo..=hi).contains(&value) {
            window_hits += 1;
            if samples.len() < 12 {
                samples.push((offset, value));
            }
        }
    }
    eprintln!("FILETIME values in window: {window_hits}");
    for (offset, value) in &samples {
        eprintln!("  offset={offset:#x} filetime={value:#x}");
    }

    // EVTX chunk magic journaled?
    let magic = b"ElfChnk\0";
    let mut magic_offsets = Vec::new();
    for (offset, window) in logfile.windows(magic.len()).enumerate() {
        if window == magic {
            magic_offsets.push(offset);
        }
    }
    eprintln!(
        "ElfChnk magic occurrences in $LogFile: {}",
        magic_offsets.len()
    );
    for offset in magic_offsets.iter().take(10) {
        eprintln!("  chunk magic at {offset:#x}");
    }

    assert!(
        window_hits > 0 || !magic_offsets.is_empty(),
        "expected journal evidence of the unflushed event-log tail"
    );
}
