use chrono::{DateTime, TimeZone, Utc};
use evidence_core::filesystem::invalid_fs_data;
use std::io;

/// Parsed MFT FILE record metadata.
#[derive(Debug, Clone)]
pub struct MftRecord {
    pub record_number: u64,
    pub sequence_number: u16,
    pub name: String,
    pub parent_ref: u64,
    pub is_dir: bool,
    pub size: u64,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub accessed_at: Option<DateTime<Utc>>,
    pub changed_at: Option<DateTime<Utc>>,
    pub hidden: bool,
    pub system: bool,
    pub deleted: bool,
    pub is_valid: bool,
}

/// NTFS FILE record parser. Extracts metadata from raw MFT record bytes.
pub struct MftRecordParser {
    bytes_per_sector: u16,
    /// Reusable buffer for fixup when record_size != 1024.
    /// For the standard 1024-byte case, a stack array is used instead.
    buf: Vec<u8>,
}

impl MftRecordParser {
    pub fn new(record_size: u32, bytes_per_sector: u16) -> Self {
        Self {
            bytes_per_sector,
            buf: vec![0u8; record_size as usize],
        }
    }

    /// Parse a single MFT FILE record into an MftRecord.
    /// Returns None for invalid records. Inactive records with a valid
    /// $FILE_NAME attribute are retained and marked as deleted.
    pub fn parse(&mut self, record: &[u8], record_number: u64) -> Option<MftRecord> {
        if record.len() < 42 || &record[0..4] != b"FILE" {
            return None;
        }

        // Stack-allocated buffer for the common 1024-byte record size,
        // avoiding per-record heap allocation entirely.
        if record.len() == 1024 {
            let mut rec = [0u8; 1024];
            rec.copy_from_slice(record);
            if apply_record_fixup(&mut rec, self.bytes_per_sector as usize).is_err() {
                return None;
            }
            return parse_mft_record(&rec, record_number);
        }

        // Fallback: reuse the pre-allocated heap buffer for non-standard sizes.
        self.buf.clear();
        self.buf.extend_from_slice(record);
        if apply_record_fixup(&mut self.buf, self.bytes_per_sector as usize).is_err() {
            return None;
        }
        parse_mft_record(&self.buf, record_number)
    }

    pub(crate) fn parse_fixed(&mut self, record: &[u8], record_number: u64) -> Option<MftRecord> {
        if record.len() < 42 || &record[0..4] != b"FILE" {
            return None;
        }
        parse_mft_record(record, record_number)
    }
}

/// Core MFT record parsing logic. Extracted from MftRecordParser to allow
/// both stack-allocated (1024-byte fast path) and heap-allocated callers.
fn parse_mft_record(rec: &[u8], record_number: u64) -> Option<MftRecord> {
    let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
    let flags = u16::from_le_bytes([rec[0x16], rec[0x17]]);
    let deleted = flags & 0x01 == 0;
    let is_dir = flags & 0x02 != 0;

    let mut name = String::new();
    let mut parent_ref: u64 = 5;
    let mut size: u64 = 0;
    let mut created_at: Option<DateTime<Utc>> = None;
    let mut modified_at: Option<DateTime<Utc>> = None;
    let mut accessed_at: Option<DateTime<Utc>> = None;
    let mut changed_at: Option<DateTime<Utc>> = None;
    let mut fn_created: Option<DateTime<Utc>> = None;
    let mut fn_modified: Option<DateTime<Utc>> = None;
    let mut fn_accessed: Option<DateTime<Utc>> = None;
    let mut fn_changed: Option<DateTime<Utc>> = None;
    let mut fn_real_size: Option<u64> = None;
    let mut fn_flags: Option<u32> = None;
    let mut selected_name_rank: Option<u8> = None;

    let mut pos = attr_off;
    while pos + 8 < rec.len() {
        let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().ok()?);
        if typ == 0xFFFFFFFF {
            break;
        }
        let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().ok()?) as usize;
        if len < 4 || pos + len > rec.len() {
            break;
        }

        match typ {
            0x10 => {
                if let Some(content) = resident_content(rec, pos, len) {
                    if content.len() >= 0x30 {
                        created_at = ntfs_to_datetime(read_ntfs_time(content, 0x00));
                        modified_at = ntfs_to_datetime(read_ntfs_time(content, 0x08));
                        changed_at = ntfs_to_datetime(read_ntfs_time(content, 0x10));
                        accessed_at = ntfs_to_datetime(read_ntfs_time(content, 0x18));
                    }
                }
            }
            0x30 => {
                if let Some(content) = resident_content(rec, pos, len) {
                    if content.len() >= 0x52 {
                        let name_len = content[0x40] as usize;
                        let name_ns = content[0x41]; // namespace: 0=POSIX, 1=Win32, 2=DOS, 3=Win32+DOS
                        let name_start = 0x42;
                        if name_len > 0 && name_start + name_len * 2 <= content.len() {
                            let chars: Vec<u16> = content[name_start..name_start + name_len * 2]
                                .chunks_exact(2)
                                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                                .collect();
                            let parsed_name = String::from_utf16_lossy(&chars);

                            // Keep name, parent ref, and timestamps from the same
                            // $FILE_NAME attribute. Real NTFS records often contain
                            // both DOS 8.3 and Win32 names; mixing name from one
                            // attribute with parent_ref from another corrupts paths.
                            let rank = file_name_namespace_rank(name_ns);
                            if selected_name_rank.is_none_or(|current| rank > current) {
                                name = parsed_name;
                                parent_ref = u64::from_le_bytes(content[0..8].try_into().ok()?)
                                    & 0x0000_FFFF_FFFF_FFFF;
                                fn_created = ntfs_to_datetime(read_ntfs_time(content, 0x08));
                                fn_modified = ntfs_to_datetime(read_ntfs_time(content, 0x10));
                                fn_changed = ntfs_to_datetime(read_ntfs_time(content, 0x18));
                                fn_accessed = ntfs_to_datetime(read_ntfs_time(content, 0x20));
                                if !is_dir && content.len() >= 0x38 {
                                    fn_real_size = Some(u64::from_le_bytes(
                                        content[0x30..0x38].try_into().ok()?,
                                    ));
                                }
                                if content.len() >= 0x3C {
                                    fn_flags = Some(u32::from_le_bytes(
                                        content[0x38..0x3C].try_into().ok()?,
                                    ));
                                }
                                selected_name_rank = Some(rank);
                            }
                        }
                    }
                }
            }
            0x80 => {
                // $DATA — file size
                if !is_unnamed_attribute(rec, pos) {
                    pos += len;
                    continue;
                }
                let is_nonresident = pos + 9 <= rec.len() && (rec[pos + 8] & 1) != 0;
                if is_nonresident {
                    if pos + 0x38 <= rec.len() {
                        size = u64::from_le_bytes(rec[pos + 0x30..pos + 0x38].try_into().ok()?);
                    }
                } else if pos + 0x14 <= rec.len() {
                    size = u32::from_le_bytes(rec[pos + 0x10..pos + 0x14].try_into().ok()?) as u64;
                }
            }
            _ => {}
        }

        if len == 0 {
            break;
        }
        pos += len;
    }

    let final_created = created_at.or(fn_created);
    let final_modified = modified_at.or(fn_modified);
    let final_accessed = accessed_at.or(fn_accessed);
    let final_changed = changed_at.or(fn_changed);

    let is_valid = if deleted {
        !name.is_empty()
    } else {
        !name.is_empty() || record_number < 24
    };
    if !is_valid {
        return None;
    }

    let size = if !is_dir && size == 0 {
        fn_real_size.unwrap_or(size)
    } else {
        size
    };

    Some(MftRecord {
        record_number,
        sequence_number: u16::from_le_bytes([rec[0x10], rec[0x11]]),
        name,
        parent_ref,
        is_dir,
        size,
        created_at: final_created,
        modified_at: final_modified,
        accessed_at: final_accessed,
        changed_at: final_changed,
        hidden: fn_flags.is_some_and(|flags| flags & 0x02 != 0),
        system: fn_flags.is_some_and(|flags| flags & 0x04 != 0),
        deleted,
        is_valid,
    })
}

fn file_name_namespace_rank(namespace: u8) -> u8 {
    match namespace {
        1 => 4, // Win32
        3 => 3, // Win32 + DOS
        0 => 2, // POSIX
        2 => 1, // DOS 8.3
        _ => 0,
    }
}

/// MFT bulk scanner. Reads MFT records sequentially in large chunks.
pub struct MftScanner {
    /// Absolute offset of the MFT in the evidence file.
    mft_abs_offset: u64,
    /// Size of each MFT record in bytes.
    record_size: u32,
    /// Total number of MFT records (from $MFT $DATA size / record_size).
    total_records: u64,
    /// Bytes per sector for fixup array.
    bytes_per_sector: u16,
}

impl MftScanner {
    pub fn new(
        volume_offset: u64,
        mft_cluster: u64,
        cluster_size: u64,
        record_size: u32,
        bytes_per_sector: u16,
        mft_data_size: u64,
    ) -> Self {
        let mft_abs_offset = volume_offset + mft_cluster.saturating_mul(cluster_size);
        let total_records = if record_size > 0 {
            mft_data_size / record_size as u64
        } else {
            0
        };
        Self {
            mft_abs_offset,
            record_size,
            total_records,
            bytes_per_sector,
        }
    }

    pub fn total_records(&self) -> u64 {
        self.total_records
    }

    pub fn record_size(&self) -> u32 {
        self.record_size
    }

    pub fn mft_abs_offset(&self) -> u64 {
        self.mft_abs_offset
    }

    /// Parse a batch of MFT records from a pre-read buffer.
    /// The buffer must contain `count * record_size` bytes starting at `start_record`.
    pub fn parse_chunk(&self, buf: &[u8], start_record: u64, count: u64) -> Vec<MftRecord> {
        let mut parser = MftRecordParser::new(self.record_size, self.bytes_per_sector);
        let mut records = Vec::with_capacity(count as usize);
        let rec_size = self.record_size as usize;

        for i in 0..count as usize {
            let offset = i * rec_size;
            if offset + rec_size > buf.len() {
                break;
            }
            let record_number = start_record + i as u64;
            if let Some(rec) = parser.parse(&buf[offset..offset + rec_size], record_number) {
                records.push(rec);
            }
        }
        records
    }
}

/// Extract resident attribute content slice.
fn resident_content(record: &[u8], attr_pos: usize, attr_len: usize) -> Option<&[u8]> {
    if attr_pos + 0x16 > record.len() {
        return None;
    }
    // Check non-resident flag
    if (record[attr_pos + 8] & 1) != 0 {
        return None;
    }
    let content_size =
        u32::from_le_bytes(record[attr_pos + 0x10..attr_pos + 0x14].try_into().ok()?) as usize;
    let content_off =
        u16::from_le_bytes(record[attr_pos + 0x14..attr_pos + 0x16].try_into().ok()?) as usize;
    let content_start = attr_pos.checked_add(content_off)?;
    let content_end = content_start.checked_add(content_size)?;
    let attr_end = attr_pos.checked_add(attr_len)?;
    if content_start >= attr_end || content_end > attr_end || content_start >= record.len() {
        return None;
    }
    record.get(content_start..content_end.min(record.len()))
}

fn is_unnamed_attribute(record: &[u8], attr_pos: usize) -> bool {
    attr_pos + 0x0a <= record.len() && record[attr_pos + 0x09] == 0
}

/// Read NTFS timestamp (100-nanosecond intervals since 1601-01-01).
fn read_ntfs_time(data: &[u8], offset: usize) -> u64 {
    if offset + 8 > data.len() {
        return 0;
    }
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap_or([0; 8]))
}

/// Convert NTFS timestamp to chrono DateTime.
fn ntfs_to_datetime(ntfs_time: u64) -> Option<DateTime<Utc>> {
    if ntfs_time == 0 {
        return None;
    }
    // NTFS epoch: 1601-01-01, Unix epoch: 1970-01-01
    // Difference in 100-nanosecond intervals: 116444736000000000
    const NTFS_EPOCH_DIFF: u64 = 116_444_736_000_000_000;
    if ntfs_time <= NTFS_EPOCH_DIFF {
        return None;
    }
    let unix_100ns = ntfs_time - NTFS_EPOCH_DIFF;
    let unix_secs = (unix_100ns / 10_000_000) as i64;
    let unix_nanos = ((unix_100ns % 10_000_000) * 100) as u32;
    Utc.timestamp_opt(unix_secs, unix_nanos).single()
}

/// Apply NTFS update sequence array fixup to a record.
fn apply_record_fixup(record: &mut [u8], sector_size: usize) -> io::Result<()> {
    if record.len() < 8 || sector_size < 2 {
        return Ok(());
    }
    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }
    let usa_bytes = usa_count * 2;
    if usa_offset + usa_bytes > record.len() {
        return Err(invalid_fs_data(
            "update sequence array exceeds record length",
        ));
    }
    let expected = [record[usa_offset], record[usa_offset + 1]];
    for i in 1..usa_count {
        let fixup_pos = i * sector_size - 2;
        if fixup_pos + 2 > record.len() {
            break;
        }
        if record[fixup_pos..fixup_pos + 2] != expected {
            return Err(invalid_fs_data("update sequence signature mismatch"));
        }
        let replacement = usa_offset + i * 2;
        record[fixup_pos] = record[replacement];
        record[fixup_pos + 1] = record[replacement + 1];
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/mft_scanner.rs"]
mod tests;
