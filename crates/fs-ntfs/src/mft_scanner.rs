use chrono::{DateTime, TimeZone, Utc};
use evidence_core::filesystem::invalid_fs_data;
use std::io;

/// Parsed MFT FILE record metadata.
#[derive(Debug, Clone)]
pub struct MftRecord {
    pub record_number: u64,
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
}

/// Core MFT record parsing logic. Extracted from MftRecordParser to allow
/// both stack-allocated (1024-byte fast path) and heap-allocated callers.
fn parse_mft_record(rec: &[u8], record_number: u64) -> Option<MftRecord> {
    let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
    let flags = u16::from_le_bytes([rec[0x16], rec[0x17]]);
    let in_use = flags & 0x01 != 0;
    let deleted = !in_use;
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
mod tests {
    use super::*;
    use chrono::Datelike;

    fn make_test_record(_record_number: u64, name: &str, parent: u64, is_dir: bool) -> Vec<u8> {
        let mut rec = vec![0u8; 1024];
        // FILE magic
        rec[0..4].copy_from_slice(b"FILE");
        // usa_offset=0, usa_count=0 → no fixup needed
        rec[4..6].copy_from_slice(&0u16.to_le_bytes());
        rec[6..8].copy_from_slice(&0u16.to_le_bytes());

        // Attribute offset
        let attr_off = 56u16;
        rec[0x14..0x16].copy_from_slice(&attr_off.to_le_bytes());

        // Flags: bit 0 = in use, bit 1 = directory
        let flags: u16 = if is_dir { 0x03 } else { 0x01 };
        rec[0x16..0x18].copy_from_slice(&flags.to_le_bytes());

        let mut pos = attr_off as usize;

        // $STANDARD_INFORMATION (0x10) — resident
        let si_content_size = 0x30u32;
        let si_attr_len = 0x60u32;
        rec[pos..pos + 4].copy_from_slice(&0x10u32.to_le_bytes());
        rec[pos + 4..pos + 8].copy_from_slice(&si_attr_len.to_le_bytes());
        rec[pos + 8] = 0; // resident
        rec[pos + 0x10..pos + 0x14].copy_from_slice(&si_content_size.to_le_bytes());
        rec[pos + 0x14..pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
        // NTFS time for ~2005-01-01: 127111680000000000
        let test_time: u64 = 127_111_680_000_000_000;
        let content_start = pos + 0x18;
        rec[content_start..content_start + 8].copy_from_slice(&test_time.to_le_bytes());
        rec[content_start + 8..content_start + 16].copy_from_slice(&test_time.to_le_bytes());
        pos += si_attr_len as usize;

        // $FILE_NAME (0x30) — resident
        let name_bytes: Vec<u16> = name.encode_utf16().collect();
        let fn_content_size = 0x52u32 + (name_bytes.len() as u32) * 2;
        let fn_attr_len = 0x18 + fn_content_size;
        rec[pos..pos + 4].copy_from_slice(&0x30u32.to_le_bytes());
        rec[pos + 4..pos + 8].copy_from_slice(&fn_attr_len.to_le_bytes());
        rec[pos + 8] = 0; // resident
        rec[pos + 0x10..pos + 0x14].copy_from_slice(&fn_content_size.to_le_bytes());
        rec[pos + 0x14..pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
        let fn_content = pos + 0x18;
        // parent_ref
        rec[fn_content..fn_content + 8].copy_from_slice(&parent.to_le_bytes());
        // timestamps
        rec[fn_content + 8..fn_content + 16].copy_from_slice(&test_time.to_le_bytes());
        rec[fn_content + 16..fn_content + 24].copy_from_slice(&test_time.to_le_bytes());
        rec[fn_content + 24..fn_content + 32].copy_from_slice(&test_time.to_le_bytes());
        rec[fn_content + 32..fn_content + 40].copy_from_slice(&test_time.to_le_bytes());
        rec[fn_content + 0x30..fn_content + 0x38].copy_from_slice(&1234u64.to_le_bytes());
        // name_len
        rec[fn_content + 0x40] = name_bytes.len() as u8;
        // name_namespace: 1 = Win32
        rec[fn_content + 0x41] = 1;
        // name (UTF-16LE)
        for (i, ch) in name_bytes.iter().enumerate() {
            let off = fn_content + 0x42 + i * 2;
            rec[off..off + 2].copy_from_slice(&ch.to_le_bytes());
        }

        // $DATA (0x80) — resident, size = 1234
        let data_attr_len = 0x18 + 0x08;
        let data_pos = pos + fn_attr_len as usize;
        if data_pos + data_attr_len <= rec.len() {
            rec[data_pos..data_pos + 4].copy_from_slice(&0x80u32.to_le_bytes());
            rec[data_pos + 4..data_pos + 8].copy_from_slice(&(data_attr_len as u32).to_le_bytes());
            rec[data_pos + 8] = 0; // resident
            rec[data_pos + 0x10..data_pos + 0x14].copy_from_slice(&1234u32.to_le_bytes());
            rec[data_pos + 0x14..data_pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
        }

        rec
    }

    fn append_file_name_attr(rec: &mut [u8], name: &str, parent: u64, namespace: u8) {
        let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFFFFFF || typ == 0 {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            pos += len;
        }

        let name_bytes: Vec<u16> = name.encode_utf16().collect();
        let fn_content_size = 0x52usize + name_bytes.len() * 2;
        let fn_attr_len = 0x18usize + fn_content_size;
        assert!(pos + fn_attr_len + 4 <= rec.len());
        rec[pos..pos + 4].copy_from_slice(&0x30u32.to_le_bytes());
        rec[pos + 4..pos + 8].copy_from_slice(&(fn_attr_len as u32).to_le_bytes());
        rec[pos + 8] = 0;
        rec[pos + 0x10..pos + 0x14].copy_from_slice(&(fn_content_size as u32).to_le_bytes());
        rec[pos + 0x14..pos + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
        let content = pos + 0x18;
        rec[content..content + 8].copy_from_slice(&parent.to_le_bytes());
        rec[content + 0x40] = name_bytes.len() as u8;
        rec[content + 0x41] = namespace;
        for (index, ch) in name_bytes.iter().enumerate() {
            let off = content + 0x42 + index * 2;
            rec[off..off + 2].copy_from_slice(&ch.to_le_bytes());
        }
        let end = pos + fn_attr_len;
        rec[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }

    fn set_first_file_name_namespace(rec: &mut [u8], namespace: u8) {
        let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if typ == 0x30 {
                if let Some(content) = resident_content(rec, pos, len) {
                    let namespace_offset = content.as_ptr() as usize - rec.as_ptr() as usize + 0x41;
                    rec[namespace_offset] = namespace;
                }
                return;
            }
            if typ == 0xFFFFFFFF || len == 0 || pos + len > rec.len() {
                return;
            }
            pos += len;
        }
    }

    fn append_named_resident_data_attr(rec: &mut [u8], name: &str, size: u32) {
        let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFF_FFFF || typ == 0 {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            pos += len;
        }

        let name_bytes: Vec<u16> = name.encode_utf16().collect();
        let name_bytes_len = name_bytes.len() * 2;
        let content_size = size as usize;
        let name_offset = 0x18usize;
        let content_offset = name_offset + name_bytes_len;
        let attr_len = content_offset + content_size;
        assert!(pos + attr_len + 4 <= rec.len());
        rec[pos..pos + 4].copy_from_slice(&0x80u32.to_le_bytes());
        rec[pos + 4..pos + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
        rec[pos + 8] = 0;
        rec[pos + 9] = name_bytes.len() as u8;
        rec[pos + 0x0a..pos + 0x0c].copy_from_slice(&(name_offset as u16).to_le_bytes());
        rec[pos + 0x10..pos + 0x14].copy_from_slice(&size.to_le_bytes());
        rec[pos + 0x14..pos + 0x16].copy_from_slice(&(content_offset as u16).to_le_bytes());
        for (index, ch) in name_bytes.iter().enumerate() {
            let off = pos + name_offset + index * 2;
            rec[off..off + 2].copy_from_slice(&ch.to_le_bytes());
        }
        let end = pos + attr_len;
        rec[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }

    fn remove_data_attrs(rec: &mut [u8]) {
        let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFF_FFFF || typ == 0 {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            if typ == 0x80 {
                rec[pos..pos + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
                break;
            }
            pos += len;
        }
    }

    #[test]
    fn parse_valid_file_record() {
        let mut parser = MftRecordParser::new(1024, 512);
        let rec = make_test_record(100, "test.txt", 5, false);
        let result = parser.parse(&rec, 100).unwrap();
        assert_eq!(result.name, "test.txt");
        assert_eq!(result.parent_ref, 5);
        assert!(!result.is_dir);
        assert!(!result.deleted);
        assert!(result.created_at.is_some());
        assert_eq!(result.size, 1234);
    }

    #[test]
    fn parse_directory_record() {
        let mut parser = MftRecordParser::new(1024, 512);
        let rec = make_test_record(200, "Users", 5, true);
        let result = parser.parse(&rec, 200).unwrap();
        assert_eq!(result.name, "Users");
        assert!(result.is_dir);
    }

    #[test]
    fn parse_multiple_file_name_attrs_keeps_selected_parent_ref() {
        let mut parser = MftRecordParser::new(1024, 512);
        let mut rec = make_test_record(300, "WINDOW~1", 5, true);
        set_first_file_name_namespace(&mut rec, 2);
        append_file_name_attr(&mut rec, "Windows", 42, 1);
        let result = parser.parse(&rec, 300).unwrap();
        assert_eq!(result.name, "Windows");
        assert_eq!(result.parent_ref, 42);
    }

    #[test]
    fn named_data_stream_does_not_override_primary_file_size() {
        let mut parser = MftRecordParser::new(1024, 512);
        let mut rec = make_test_record(301, "System.evtx", 5, false);
        append_named_resident_data_attr(&mut rec, "Zone.Identifier", 0);
        let result = parser.parse(&rec, 301).unwrap();
        assert_eq!(result.size, 1234);
    }

    #[test]
    fn file_name_real_size_is_fallback_when_data_attr_unavailable() {
        let mut parser = MftRecordParser::new(1024, 512);
        let mut rec = make_test_record(302, "SOFTWARE", 5, false);
        remove_data_attrs(&mut rec);
        let result = parser.parse(&rec, 302).unwrap();
        assert_eq!(result.size, 1234);
    }

    #[test]
    fn parse_invalid_record() {
        let mut parser = MftRecordParser::new(1024, 512);
        let mut rec = vec![0u8; 1024];
        rec[0..4].copy_from_slice(b"BAAD");
        assert!(parser.parse(&rec, 0).is_none());
    }

    #[test]
    fn parse_inactive_record() {
        let mut parser = MftRecordParser::new(1024, 512);
        let mut rec = make_test_record(500, "deleted.txt", 5, false);
        rec[0x16] = 0x00;
        let result = parser.parse(&rec, 500).unwrap();
        assert_eq!(result.name, "deleted.txt");
        assert!(result.deleted);
        assert!(!result.is_dir);
    }

    #[test]
    fn parse_inactive_hidden_system_record() {
        let mut parser = MftRecordParser::new(1024, 512);
        let mut rec = make_test_record(501, "hidden-deleted.txt", 5, false);
        rec[0x16] = 0x00;
        let mut pos = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if typ == 0x30 {
                if let Some(content) = resident_content(&rec, pos, len) {
                    let flags_offset = content.as_ptr() as usize - rec.as_ptr() as usize + 0x38;
                    rec[flags_offset..flags_offset + 4].copy_from_slice(&0x06u32.to_le_bytes());
                }
                break;
            }
            if typ == 0xFFFFFFFF || len == 0 || pos + len > rec.len() {
                break;
            }
            pos += len;
        }

        let result = parser.parse(&rec, 501).unwrap();
        assert!(result.deleted);
        assert!(result.hidden);
        assert!(result.system);
    }

    #[test]
    fn scanner_parse_chunk() {
        let scanner = MftScanner::new(0, 0, 4096, 1024, 512, 1024 * 100);
        let mut buf = Vec::new();
        for _ in 0..10 {
            buf.extend_from_slice(&make_test_record(0, "file.txt", 5, false));
        }
        let records = scanner.parse_chunk(&buf, 0, 10);
        assert_eq!(records.len(), 10);
        assert_eq!(records[0].name, "file.txt");
    }

    #[test]
    fn ntfs_time_conversion() {
        let ntfs_time: u64 = 127_111_680_000_000_000;
        let dt = ntfs_to_datetime(ntfs_time).unwrap();
        assert_eq!(dt.year(), 2003);
    }

    #[test]
    fn zero_time_returns_none() {
        assert!(ntfs_to_datetime(0).is_none());
    }
}
