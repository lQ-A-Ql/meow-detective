/// Fixture builders that generate format-correct binary test data.
/// These match known Windows artifact binary structures.
use chrono::{DateTime, Utc};

/// Build a Prefetch v30 file (Windows 10/11 format)
pub fn build_prefetch_v30(exe_name: &str, run_count: u32, last_runs: &[DateTime<Utc>]) -> Vec<u8> {
    let mut data = Vec::new();
    // Magic (offset 0x00, 4 bytes)
    data.extend_from_slice(b"SCCA");
    // Format version (0x04, 4 bytes) - v30 = 0x1E
    data.extend_from_slice(&0x1Eu32.to_le_bytes());
    // Signature (0x08, 4 bytes) - v30 has MAM\x04 at different offset, but many files have 0 here
    data.extend_from_slice(&0u32.to_le_bytes());
    // Unused (0x0C, 4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());
    // File size (0x10, 4 bytes) - uncompressed size of the original executable
    data.extend_from_slice(&0x0000A000u32.to_le_bytes());

    // Executable name (0x14, 60 bytes, UTF-16LE null-terminated)
    let mut name_buf = vec![0u8; 60];
    for (i, c) in exe_name.encode_utf16().enumerate() {
        if i * 2 + 1 < 60 {
            name_buf[i * 2] = (c & 0xFF) as u8;
            name_buf[i * 2 + 1] = ((c >> 8) & 0xFF) as u8;
        }
    }
    data.extend_from_slice(&name_buf);

    // Hash (0x50, 4 bytes) - PE hash of executable
    data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
    // Flags (0x54, 4 bytes) - 0 = uncompressed
    data.extend_from_slice(&0u32.to_le_bytes());

    // After header: volume info, directory info, etc.
    // For v30: skip 12 bytes then run_count
    let skip = vec![0u8; 12];
    data.extend_from_slice(&skip);

    // Run count (4 bytes)
    data.extend_from_slice(&run_count.to_le_bytes());

    // 8 FILETIME slots (64 bytes total)
    for i in 0..8 {
        if i < last_runs.len() {
            let ft = dt_to_filetime(last_runs[i]);
            data.extend_from_slice(&ft.to_le_bytes());
        } else {
            data.extend_from_slice(&0u64.to_le_bytes());
        }
    }

    // Fill remaining (referenced file strings, etc.) with zeros to make a realistic size
    let target_size = 4096;
    while data.len() < target_size {
        data.push(0);
    }

    data
}

/// Build a Windows Shell Link (.lnk) file with optional LinkInfo
pub fn build_lnk(
    target_path: Option<&str>,
    creation_time: Option<DateTime<Utc>>,
    write_time: Option<DateTime<Utc>>,
    file_size: u32,
) -> Vec<u8> {
    let mut data = Vec::new();

    // Header size (0x4C = 76 bytes, standard)
    data.extend_from_slice(&0x4Cu32.to_le_bytes());

    // Link CLSID: {00021401-0000-0000-C000-000000000046} (16 bytes)
    let clsid: [u8; 16] = [
        0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x46,
    ];
    data.extend_from_slice(&clsid);

    // LinkFlags: set bits based on what we include
    let mut flags = 0u32;
    let has_linkinfo = target_path.is_some();
    if has_linkinfo {
        flags |= 0x00000002; // HAS_LINK_INFO
    }
    data.extend_from_slice(&flags.to_le_bytes());

    // FileAttributes (4 bytes) - FILE_ATTRIBUTE_NORMAL
    data.extend_from_slice(&0x80u32.to_le_bytes());

    // Creation time (8 bytes FILETIME)
    let ct = creation_time.map(dt_to_filetime).unwrap_or(0);
    data.extend_from_slice(&ct.to_le_bytes());

    // Access time (8 bytes FILETIME)
    data.extend_from_slice(&ct.to_le_bytes());

    // Write time (8 bytes FILETIME)
    let wt = write_time.map(dt_to_filetime).unwrap_or(0);
    data.extend_from_slice(&wt.to_le_bytes());

    // File size (4 bytes)
    data.extend_from_slice(&file_size.to_le_bytes());

    // Icon index (4 bytes)
    data.extend_from_slice(&0i32.to_le_bytes());

    // Show command (4 bytes) - SW_NORMAL
    data.extend_from_slice(&1u32.to_le_bytes());

    // HotKey (2 bytes)
    data.extend_from_slice(&[0u8; 2]);

    // Reserved (10 bytes)
    data.extend_from_slice(&[0u8; 10]);

    // --- Optional sections beyond the fixed header ---

    // LinkInfo (if target_path is provided)
    if let Some(path) = target_path {
        let path_bytes = path.as_bytes();
        // LinkInfo structure:
        // LinkInfoSize (4), LinkInfoHeaderSize (4), LinkInfoFlags (4),
        // VolumeIDOffset (4), LocalBasePathOffset (4), ...
        // We'll embed the path as local_base_path

        let volume_id_offset = 16u32; // 4 fields × 4 bytes = 16
        let local_base_path_offset = volume_id_offset + 16; // 16 bytes for VolumeID placeholder

        let linkinfo_size = local_base_path_offset + path_bytes.len() as u32 + 1;

        data.extend_from_slice(&linkinfo_size.to_le_bytes());
        data.extend_from_slice(&0x1Cu32.to_le_bytes()); // LinkInfoHeaderSize
        data.extend_from_slice(&volume_id_offset.to_le_bytes());
        data.extend_from_slice(&local_base_path_offset.to_le_bytes());

        // VolumeID (minimal: 16 bytes of zeros)
        data.extend_from_slice(&[0u8; 16]);

        // Local base path (null-terminated)
        data.extend_from_slice(path_bytes);
        data.push(0);
    }

    data
}

/// Build a Recycle Bin $I file (v2 format)
pub fn build_recycle_bin_i(
    original_path: &str,
    file_size: u64,
    deletion_time: DateTime<Utc>,
) -> Vec<u8> {
    let mut data = Vec::new();

    // Header size (8 bytes) - v2 = 0x20 (32)
    data.extend_from_slice(&0x20u64.to_le_bytes());

    // Physical file size (8 bytes)
    data.extend_from_slice(&file_size.to_le_bytes());

    // Deletion time (8 bytes FILETIME)
    let ft = dt_to_filetime(deletion_time);
    data.extend_from_slice(&ft.to_le_bytes());

    // Remaining header (to reach 0x20): 32 - 24 = 8 bytes of zeros
    data.extend_from_slice(&[0u8; 8]);

    // Original path (UTF-16LE null-terminated)
    for c in original_path.encode_utf16() {
        data.extend_from_slice(&c.to_le_bytes());
    }
    data.extend_from_slice(&[0u8, 0u8]);

    data
}

/// Build a minimal valid Registry hive (regf format)
pub fn build_registry_hive(hive_name: &str, last_written: DateTime<Utc>) -> Vec<u8> {
    let mut data = Vec::new();

    // Magic (4 bytes) - "regf"
    data.extend_from_slice(b"regf");

    // Primary sequence number (4 bytes)
    data.extend_from_slice(&1u32.to_le_bytes());
    // Secondary sequence number (4 bytes)
    data.extend_from_slice(&1u32.to_le_bytes());

    // Last written timestamp (8 bytes FILETIME)
    let ft = dt_to_filetime(last_written);
    data.extend_from_slice(&ft.to_le_bytes());

    // Major version (4 bytes) = 1
    data.extend_from_slice(&1u32.to_le_bytes());
    // Minor version (4 bytes) = 3 or 5 for Win10
    data.extend_from_slice(&5u32.to_le_bytes());

    // File type (4 bytes) - 0 = normal
    data.extend_from_slice(&0u32.to_le_bytes());
    // File format (4 bytes) - 1 = direct memory load
    data.extend_from_slice(&1u32.to_le_bytes());

    // Root cell offset (4 bytes) - relative to first HBIN
    // HBIN starts at 0x1000 (4096). Root cell at offset 0x20 within HBIN.
    data.extend_from_slice(&0x20u32.to_le_bytes());

    // Hbin data size (4 bytes) - can be 0 for base block
    data.extend_from_slice(&0u32.to_le_bytes());

    // Clustered index (4 bytes) - non-zero for non-primary files
    data.extend_from_slice(&1u32.to_le_bytes());

    // File name (64 bytes, UTF-16LE)
    let mut name_buf = vec![0u8; 64];
    for (i, c) in hive_name.encode_utf16().enumerate() {
        if i * 2 + 1 < 64 {
            name_buf[i * 2] = (c & 0xFF) as u8;
            name_buf[i * 2 + 1] = ((c >> 8) & 0xFF) as u8;
        }
    }
    data.extend_from_slice(&name_buf);

    // Reserved (32 bytes) - can be 0
    data.extend_from_slice(&[0u8; 32]);

    // Checksum (4 bytes) - we'll skip the actual calculation and use 0
    data.extend_from_slice(&0u32.to_le_bytes());

    // Reserved (3968 bytes) to reach 0x1000 (4096 total base block)
    let remaining = 0x1000usize.saturating_sub(data.len());
    data.extend_from_slice(&vec![0u8; remaining]);

    // --- HBIN block at 0x1000 ---
    // HBIN magic (4 bytes) - "hbin"
    data.extend_from_slice(b"hbin");

    // Offset of first HBIN (4 bytes) - 0 for primary
    data.extend_from_slice(&0u32.to_le_bytes());

    // Offset of next HBIN (4 bytes) - 0 (no next)
    data.extend_from_slice(&0u32.to_le_bytes());

    // HBIN size (4 bytes) - e.g. 4096
    let hbin_size = 4096u32;
    data.extend_from_slice(&hbin_size.to_le_bytes());

    // --- NK cell at offset 0x20 within HBIN (0x1020 absolute) ---
    // Cell size (4 bytes, signed) - negative = allocated, positive = free
    let cell_size = -80i32; // 80 bytes allocated
    data.extend_from_slice(&cell_size.to_le_bytes());
    // Signature (2 bytes) - "nk"
    data.extend_from_slice(b"nk");
    // Flags (2 bytes) - 0x2C = key is root + has class name
    data.extend_from_slice(&0x2Cu16.to_le_bytes());
    // Last written timestamp (8 bytes)
    data.extend_from_slice(&ft.to_le_bytes());

    // Parent cell offset (4 bytes) - 0xFFFFFFFF for root
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    // Subkeys count (4 bytes) - 0
    data.extend_from_slice(&0u32.to_le_bytes());
    // Subkeys list offset (4 bytes) - 0xFFFFFFFF
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    // Values list offset (4 bytes) - 0xFFFFFFFF
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    // Security key offset (4 bytes) - 0xFFFFFFFF
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    // Class name offset (4 bytes) - 0xFFFFFFFF
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());

    // Max subkey name length (4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());
    // Max class name length (4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());
    // Max value name length (4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());
    // Max value data size (4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());
    // Work var (4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());

    // Key name length (2 bytes) - in bytes (not chars) for UTF-16LE
    let name_bytes = hive_name.len() * 2;
    data.extend_from_slice(&(name_bytes as u16).to_le_bytes());
    // Class name length (2 bytes) - 0
    data.extend_from_slice(&0u16.to_le_bytes());

    // Key name (UTF-16LE)
    for c in hive_name.encode_utf16() {
        data.extend_from_slice(&c.to_le_bytes());
    }

    // Fill remaining HBIN space
    while data.len() < 0x2000 {
        data.push(0);
    }

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
