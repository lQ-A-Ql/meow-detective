const FOXMAIL_INDEX_MAGIC: &[u8; 4] = b"FXIS";
const FOXMAIL_LIST_MAGIC: &[u8; 4] = b"LSTG";
const INDEX_HEADER_SIZE: usize = 0x200;
const INDEX_RECORD_SIZE: usize = 0x200;
const INDEX_TEXT_OFFSET: usize = 0x33;

/// One Foxmail 7 `Mails/Index` record.
#[derive(Debug, Clone, PartialEq)]
pub struct FoxmailIndexRecord {
    pub id: u32,
    pub from_name: String,
    pub from_email: String,
    pub to_name: String,
    pub to_email: String,
    pub subject: String,
    pub primary_time: Option<chrono::DateTime<chrono::Utc>>,
    pub secondary_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// Parsed Foxmail 7 index metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct FoxmailIndex {
    pub records: Vec<FoxmailIndexRecord>,
    pub invalid_record_count: usize,
}

/// Return whether bytes identify a Foxmail 7 index file.
pub fn is_index(data: &[u8]) -> bool {
    data.len() >= INDEX_HEADER_SIZE
        && data.starts_with(FOXMAIL_INDEX_MAGIC)
        && data.get(4..8) == Some(&[2, 0, 1, 0])
}

/// Return whether bytes identify Foxmail's list/index container (`LSTG`).
pub fn is_list_container(data: &[u8]) -> bool {
    data.starts_with(FOXMAIL_LIST_MAGIC)
}

/// Parse the fixed-size records in a Foxmail 7 `Mails/Index` file.
pub fn parse_index(data: &[u8]) -> FoxmailIndex {
    if !is_index(data) {
        return FoxmailIndex {
            records: Vec::new(),
            invalid_record_count: 0,
        };
    }
    let mut records = Vec::new();
    let mut invalid_record_count = 0;
    for record in data[INDEX_HEADER_SIZE..].as_chunks::<INDEX_RECORD_SIZE>().0 {
        if record[..4].iter().all(|byte| *byte == 0) {
            continue;
        }
        match parse_index_record(record) {
            Some(record) => records.push(record),
            None => invalid_record_count += 1,
        }
    }
    FoxmailIndex {
        records,
        invalid_record_count,
    }
}

fn parse_index_record(record: &[u8]) -> Option<FoxmailIndexRecord> {
    let id = u32::from_le_bytes(record.get(..4)?.try_into().ok()?);
    let lengths = record.get(0x2d..0x32)?;
    let total_text = lengths
        .iter()
        .map(|length| usize::from(*length))
        .sum::<usize>();
    let text = record.get(INDEX_TEXT_OFFSET..INDEX_TEXT_OFFSET + total_text)?;
    let mut fields = Vec::with_capacity(5);
    let mut offset = 0;
    for length in lengths {
        let end = offset + usize::from(*length);
        fields.push(String::from_utf8(text.get(offset..end)?.to_vec()).ok()?);
        offset = end;
    }
    Some(FoxmailIndexRecord {
        id,
        from_name: fields[0].clone(),
        from_email: fields[1].clone(),
        to_name: fields[2].clone(),
        to_email: fields[3].clone(),
        subject: fields[4].clone(),
        primary_time: parse_ole_datetime(record.get(0x08..0x10)?),
        secondary_time: parse_ole_datetime(record.get(0x10..0x18)?),
    })
}

fn parse_ole_datetime(bytes: &[u8]) -> Option<chrono::DateTime<chrono::Utc>> {
    let value = f64::from_le_bytes(bytes.try_into().ok()?);
    if !value.is_finite() || !(0.0..=100_000.0).contains(&value) {
        return None;
    }
    let micros = (value * 86_400.0 * 1_000_000.0).round();
    if micros > i64::MAX as f64 {
        return None;
    }
    let epoch = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    let timestamp = epoch.checked_add_signed(chrono::Duration::microseconds(micros as i64))?;
    Some(chrono::DateTime::from_naive_utc_and_offset(
        timestamp,
        chrono::Utc,
    ))
}
