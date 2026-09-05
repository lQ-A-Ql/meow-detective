//! Synthetic binary fixtures for Windows artifact extractors.
//!
//! These helpers generate format-correct byte sequences for unit/integration
//! tests. They are intentionally self-contained and mirror the builders used by
//! `artifacts-windows` tests so that downstream crates can validate extractor
//! behaviour without relying on real disk samples.

use chrono::{DateTime, Utc};

/// Build a Prefetch v30 file (Windows 10/11 format).
pub fn build_prefetch_v30(exe_name: &str, run_count: u32, last_runs: &[DateTime<Utc>]) -> Vec<u8> {
    let mut data = Vec::new();
    // Format version (0x00, 4 bytes) - v30 = 0x1E
    data.extend_from_slice(&0x1Eu32.to_le_bytes());
    // Signature (0x04, 4 bytes)
    data.extend_from_slice(b"SCCA");
    // Unknown (0x08, 4 bytes)
    data.extend_from_slice(&0x11u32.to_le_bytes());
    // File size (0x0C, 4 bytes)
    data.extend_from_slice(&0x0000A000u32.to_le_bytes());

    // Executable name (0x10, 60 bytes, UTF-16LE null-terminated)
    let mut name_buf = vec![0u8; 60];
    for (i, c) in exe_name.encode_utf16().enumerate() {
        if i * 2 + 1 < 60 {
            name_buf[i * 2] = (c & 0xFF) as u8;
            name_buf[i * 2 + 1] = ((c >> 8) & 0xFF) as u8;
        }
    }
    data.extend_from_slice(&name_buf);

    // Hash (0x4C, 4 bytes)
    data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
    // Flags (0x50, 4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());

    // File information v30 variant 2 (212 bytes) starts at 0x54.
    let mut file_info = vec![0u8; 212];
    file_info[0..4].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[8..12].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[16..20].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[24..28].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[116..120].copy_from_slice(&run_count.to_le_bytes());
    file_info[120..124].copy_from_slice(&1u32.to_le_bytes());
    file_info[124..128].copy_from_slice(&3u32.to_le_bytes());
    file_info[128..132].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[132..136].copy_from_slice(&0u32.to_le_bytes());

    for i in 0..8 {
        let offset = 44 + i * 8;
        if i < last_runs.len() {
            let ft = dt_to_filetime(last_runs[i]);
            file_info[offset..offset + 8].copy_from_slice(&ft.to_le_bytes());
        } else {
            file_info[offset..offset + 8].copy_from_slice(&0u64.to_le_bytes());
        }
    }
    data.extend_from_slice(&file_info);

    let target_size = 4096;
    while data.len() < target_size {
        data.push(0);
    }

    data
}

/// Build a Windows Shell Link (.lnk) file with optional LinkInfo.
pub fn build_lnk(
    target_path: Option<&str>,
    creation_time: Option<DateTime<Utc>>,
    write_time: Option<DateTime<Utc>>,
    file_size: u32,
) -> Vec<u8> {
    let mut data = Vec::new();

    // Header size
    data.extend_from_slice(&0x4Cu32.to_le_bytes());

    // Link CLSID: {00021401-0000-0000-C000-000000000046}
    let clsid: [u8; 16] = [
        0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x46,
    ];
    data.extend_from_slice(&clsid);

    let mut flags = 0u32;
    let has_linkinfo = target_path.is_some();
    if has_linkinfo {
        flags |= 0x00000002; // HAS_LINK_INFO
    }
    data.extend_from_slice(&flags.to_le_bytes());

    data.extend_from_slice(&0x80u32.to_le_bytes()); // FileAttributes

    let ct = creation_time.map(dt_to_filetime).unwrap_or(0);
    data.extend_from_slice(&ct.to_le_bytes());
    data.extend_from_slice(&ct.to_le_bytes()); // Access time

    let wt = write_time.map(dt_to_filetime).unwrap_or(0);
    data.extend_from_slice(&wt.to_le_bytes());

    data.extend_from_slice(&file_size.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes()); // Icon index
    data.extend_from_slice(&1u32.to_le_bytes()); // Show command
    data.extend_from_slice(&[0u8; 2]); // HotKey
    data.extend_from_slice(&[0u8; 10]); // Reserved

    if let Some(path) = target_path {
        let path_bytes = path.as_bytes();
        let volume_id_offset = 16u32;
        let local_base_path_offset = volume_id_offset + 16;
        let linkinfo_size = local_base_path_offset + path_bytes.len() as u32 + 1;

        data.extend_from_slice(&linkinfo_size.to_le_bytes());
        data.extend_from_slice(&0x1Cu32.to_le_bytes()); // LinkInfoHeaderSize
        data.extend_from_slice(&volume_id_offset.to_le_bytes());
        data.extend_from_slice(&local_base_path_offset.to_le_bytes());
        data.extend_from_slice(&[0u8; 16]); // VolumeID placeholder
        data.extend_from_slice(path_bytes);
        data.push(0);
    }

    data
}

/// Build a Recycle Bin $I file (v2 format).
pub fn build_recycle_bin_i(
    original_path: &str,
    file_size: u64,
    deletion_time: DateTime<Utc>,
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x20u64.to_le_bytes()); // Header size
    data.extend_from_slice(&file_size.to_le_bytes());
    let ft = dt_to_filetime(deletion_time);
    data.extend_from_slice(&ft.to_le_bytes());
    data.extend_from_slice(&[0u8; 8]); // Remaining header

    for c in original_path.encode_utf16() {
        data.extend_from_slice(&c.to_le_bytes());
    }
    data.extend_from_slice(&[0u8, 0u8]);

    data
}

fn dt_to_filetime(dt: DateTime<Utc>) -> u64 {
    let secs = dt.timestamp();
    let nanos = dt.timestamp_subsec_nanos() as u64;
    if secs >= 0 {
        (secs as u64 + 11_644_473_600u64) * 10_000_000u64 + nanos / 100
    } else {
        0
    }
}
