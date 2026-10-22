//! Foxmail 7 mailbox framing and index metadata.
//!
//! Foxmail stores messages in a binary mailbox container rather than in a
//! standards-compliant mbox stream. The records are separated by the marker
//! used by the desktop client; the RFC822 payload inside each record is left
//! untouched so the application layer can reuse its normal EML parser.

const RECORD_SEPARATOR: [u8; 16] = [
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, b'S', b'\r',
    b'\n',
];
const FOXMAIL_INDEX_MAGIC: &[u8; 4] = b"FXIS";
const FOXMAIL_LIST_MAGIC: &[u8; 4] = b"LSTG";
const INDEX_HEADER_SIZE: usize = 0x200;
const INDEX_RECORD_SIZE: usize = 0x200;
const INDEX_TEXT_OFFSET: usize = 0x33;
const FOXMAIL_V6_PASSWORD_KEY: &[u8; 8] = b"~draGon~";
const FOXMAIL_V7_PASSWORD_KEY: &[u8; 8] = b"~F@7%m$~";

/// Foxmail account-file generation used by the `Account.rec0` password codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoxmailAccountVersion {
    V6,
    V7,
}

/// A password recovered from a Foxmail account record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoxmailAccountPassword {
    pub version: FoxmailAccountVersion,
    pub password: String,
}

/// Non-secret account metadata stored in Foxmail `Account.rec0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoxmailAccountInfo {
    pub account: Option<String>,
    pub email: Option<String>,
    pub incoming_server: Option<String>,
    pub outgoing_server: Option<String>,
    pub incoming_port: Option<u16>,
    pub outgoing_port: Option<u16>,
    pub incoming_ssl: Option<bool>,
    pub outgoing_ssl: Option<bool>,
}

/// Errors returned while locating or decoding an `Account.rec0` password.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FoxmailPasswordError {
    #[error("Foxmail account file is empty")]
    EmptyAccountFile,
    #[error("unsupported Foxmail account version marker 0x{0:02x}")]
    UnsupportedVersion(u8),
    #[error("Foxmail account password field is missing")]
    PasswordFieldMissing,
    #[error("Foxmail account password field is truncated")]
    TruncatedPasswordField,
    #[error("Foxmail account password field is not valid hexadecimal")]
    InvalidHex,
    #[error("Foxmail account password field has an invalid length")]
    InvalidPasswordLength,
    #[error("Foxmail account password is not valid UTF-8")]
    InvalidUtf8,
}

/// Decode the password stored in Foxmail `Account.rec0`/V7 account records.
///
/// The record stores an ASCII hexadecimal byte string after a `Password`
/// marker. The V7 transform is the historical Foxmail algorithm: adjust the
/// first byte with the key-byte sum, XOR the remaining bytes with the cyclic
/// key, then subtract the adjusted bytes with Foxmail's 255-based underflow
/// rule.
pub fn parse_account_password(data: &[u8]) -> Result<FoxmailAccountPassword, FoxmailPasswordError> {
    let version = account_version(data)?;
    let encoded = find_latest_password_field(data)?;
    let password = decode_account_password(version, encoded)?;
    Ok(FoxmailAccountPassword { version, password })
}

/// Decode one extracted Foxmail password field without parsing the container.
pub fn decode_account_password(
    version: FoxmailAccountVersion,
    encoded_hex: &[u8],
) -> Result<String, FoxmailPasswordError> {
    if encoded_hex.is_empty() || encoded_hex.len() % 2 != 0 {
        return Err(FoxmailPasswordError::InvalidPasswordLength);
    }
    let mut encrypted = Vec::with_capacity(encoded_hex.len() / 2);
    for pair in encoded_hex.chunks_exact(2) {
        let high = hex_nibble(pair[0]).ok_or(FoxmailPasswordError::InvalidHex)?;
        let low = hex_nibble(pair[1]).ok_or(FoxmailPasswordError::InvalidHex)?;
        encrypted.push((high << 4) | low);
    }
    if encrypted.len() < 2 {
        return Err(FoxmailPasswordError::InvalidPasswordLength);
    }
    let key = match version {
        FoxmailAccountVersion::V6 => FOXMAIL_V6_PASSWORD_KEY,
        FoxmailAccountVersion::V7 => FOXMAIL_V7_PASSWORD_KEY,
    };
    let first_adjustment = key.iter().map(|byte| usize::from(*byte)).sum::<usize>() % 255;
    let mut adjusted = encrypted.clone();
    adjusted[0] ^= first_adjustment as u8;
    let mut plaintext = Vec::with_capacity(encrypted.len() - 1);
    for index in 0..encrypted.len() - 1 {
        let mixed = encrypted[index + 1] ^ key[index % key.len()];
        let adjusted_byte = adjusted[index];
        let value = if mixed < adjusted_byte {
            mixed.wrapping_add(255).wrapping_sub(adjusted_byte)
        } else {
            mixed - adjusted_byte
        };
        plaintext.push(value);
    }
    String::from_utf8(plaintext).map_err(|_| FoxmailPasswordError::InvalidUtf8)
}

/// Parse the non-secret account fields from a Foxmail `Account.rec0` file.
pub fn parse_account_info(data: &[u8]) -> Option<FoxmailAccountInfo> {
    if data.is_empty() {
        return None;
    }
    let account = latest_string_field(data, b"Account");
    let email = latest_string_field(data, b"Email");
    let incoming_server = latest_string_field(data, b"IncomingServer");
    let outgoing_server = latest_string_field(data, b"OutgoingServer");
    let incoming_port =
        latest_u32_field(data, b"IncomingPort").and_then(|value| u16::try_from(value).ok());
    let outgoing_port =
        latest_u32_field(data, b"OutgoingPort").and_then(|value| u16::try_from(value).ok());
    let incoming_ssl = latest_u32_field(data, b"InComingSSL").map(|value| value != 0);
    let outgoing_ssl = latest_u32_field(data, b"OutgoingSSL").map(|value| value != 0);
    (account.is_some() || email.is_some() || incoming_server.is_some() || outgoing_server.is_some())
        .then_some(FoxmailAccountInfo {
            account,
            email,
            incoming_server,
            outgoing_server,
            incoming_port,
            outgoing_port,
            incoming_ssl,
            outgoing_ssl,
        })
}

fn account_version(data: &[u8]) -> Result<FoxmailAccountVersion, FoxmailPasswordError> {
    match data.first().copied() {
        None => Err(FoxmailPasswordError::EmptyAccountFile),
        Some(0xd0) => Ok(FoxmailAccountVersion::V6),
        Some(0x52) => Ok(FoxmailAccountVersion::V7),
        Some(marker) => Err(FoxmailPasswordError::UnsupportedVersion(marker)),
    }
}

fn latest_string_field(data: &[u8], key: &[u8]) -> Option<String> {
    latest_field(data, key).and_then(|value| String::from_utf8(value.to_vec()).ok())
}

fn latest_u32_field(data: &[u8], key: &[u8]) -> Option<u32> {
    latest_scalar_field(data, key)
        .and_then(|value| value.get(..4)?.try_into().ok().map(u32::from_le_bytes))
}

fn latest_scalar_field<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut latest = None;
    let mut offset = 0;
    while let Some(relative) = data
        .get(offset..)?
        .windows(key.len())
        .position(|window| window == key)
    {
        let key_start = offset + relative;
        let key_length = data
            .get(key_start.checked_sub(4)?..key_start)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes);
        if key_length != Some(key.len() as u32) {
            offset = key_start + 1;
            continue;
        }
        let value_start = key_start + key.len() + 4;
        latest = Some(data.get(value_start..value_start + 4)?);
        offset = value_start + 4;
    }
    latest
}

fn latest_field<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut latest = None;
    let mut offset = 0;
    while let Some(relative) = data
        .get(offset..)?
        .windows(key.len())
        .position(|window| window == key)
    {
        let key_start = offset + relative;
        let key_length = data
            .get(key_start.checked_sub(4)?..key_start)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes);
        if key_length != Some(key.len() as u32) {
            offset = key_start + 1;
            continue;
        }
        let value_header = key_start.checked_add(key.len())?;
        let value_length = u32::from_le_bytes(
            data.get(value_header + 4..value_header + 8)?
                .try_into()
                .ok()?,
        ) as usize;
        let value_start = value_header + 8;
        let value_end = value_start.checked_add(value_length)?;
        latest = Some(data.get(value_start..value_end)?);
        offset = value_end;
    }
    latest
}

fn find_latest_password_field(data: &[u8]) -> Result<&[u8], FoxmailPasswordError> {
    const MARKER: &[u8] = b"Password";
    let mut latest = None;
    for index in 1..data.len().saturating_sub(MARKER.len()) {
        if data[index - 1] != 0 || data.get(index..index + MARKER.len()) != Some(MARKER) {
            continue;
        }
        let length_offset = index + MARKER.len() + 4;
        let length_bytes = data
            .get(length_offset..length_offset + 4)
            .ok_or(FoxmailPasswordError::TruncatedPasswordField)?;
        let length = u32::from_le_bytes(
            length_bytes
                .try_into()
                .map_err(|_| FoxmailPasswordError::TruncatedPasswordField)?,
        ) as usize;
        let value_start = length_offset + 4;
        let value_end = value_start
            .checked_add(length)
            .ok_or(FoxmailPasswordError::TruncatedPasswordField)?;
        latest = Some(
            data.get(value_start..value_end)
                .ok_or(FoxmailPasswordError::TruncatedPasswordField)?,
        );
    }
    latest.ok_or(FoxmailPasswordError::PasswordFieldMissing)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// One Foxmail 7 `Mails/Index` record. Foxmail stores the mail body in a
/// separate private binary file; this record contains the recoverable header
/// metadata used by the client list view.
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
    for record in data[INDEX_HEADER_SIZE..].chunks_exact(INDEX_RECORD_SIZE) {
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

/// Result of splitting a Foxmail mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoxmailBox {
    /// RFC822 payloads that can be passed to the regular EML parser.
    pub records: Vec<Vec<u8>>,
    /// Number of Foxmail record separators observed in the source.
    pub separator_count: usize,
    /// Number of bounded records that did not contain an RFC822 header block.
    pub ignored_record_count: usize,
}

/// Split a Foxmail `.box` file into bounded RFC822 payloads.
///
/// A separator is required before a file is treated as a native Foxmail box.
/// This prevents arbitrary `.box` files from being interpreted as mail. A
/// trailing record without a separator is retained, because Foxmail can leave
/// the final record unterminated after an interrupted write.
pub fn parse_box(data: &[u8]) -> FoxmailBox {
    let positions = separator_positions(data);
    if positions.is_empty() {
        return FoxmailBox {
            records: Vec::new(),
            separator_count: 0,
            ignored_record_count: 0,
        };
    }

    let mut records = Vec::new();
    let mut ignored_record_count = 0;
    let mut segment_start = 0;
    for (separator_index, separator_start) in positions.iter().copied().enumerate() {
        collect_record(
            &data[segment_start..separator_start],
            &mut records,
            &mut ignored_record_count,
            separator_index > 0,
        );
        segment_start = separator_start + RECORD_SEPARATOR.len();
    }
    collect_record(
        &data[segment_start..],
        &mut records,
        &mut ignored_record_count,
        true,
    );

    FoxmailBox {
        records,
        separator_count: positions.len(),
        ignored_record_count,
    }
}

fn separator_positions(data: &[u8]) -> Vec<usize> {
    data.windows(RECORD_SEPARATOR.len())
        .enumerate()
        .filter_map(|(index, window)| (window == RECORD_SEPARATOR).then_some(index))
        .collect()
}

fn collect_record(
    segment: &[u8],
    records: &mut Vec<Vec<u8>>,
    ignored_record_count: &mut usize,
    count_invalid: bool,
) {
    let trimmed = trim_record_padding(segment);
    if trimmed.is_empty() {
        return;
    }
    let Some(header_start) = find_header_start(trimmed) else {
        if count_invalid {
            *ignored_record_count += 1;
        }
        return;
    };
    let payload = trim_record_padding(&trimmed[header_start..]);
    if payload.is_empty() {
        if count_invalid {
            *ignored_record_count += 1;
        }
    } else {
        records.push(payload.to_vec());
    }
}

fn trim_record_padding(mut data: &[u8]) -> &[u8] {
    while data
        .first()
        .is_some_and(|byte| *byte == 0 || *byte == b'\r' || *byte == b'\n')
    {
        data = &data[1..];
    }
    while data
        .last()
        .is_some_and(|byte| *byte == 0 || *byte == b'\r' || *byte == b'\n')
    {
        data = &data[..data.len() - 1];
    }
    data
}

fn find_header_start(data: &[u8]) -> Option<usize> {
    let mut line_start = 0;
    while line_start < data.len() {
        let line_end = data[line_start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| line_start + offset)
            .unwrap_or(data.len());
        let line = data.get(line_start..line_end)?;
        if is_rfc822_header_line(line) {
            return Some(line_start);
        }
        if line_end == data.len() {
            break;
        }
        line_start = line_end + 1;
    }
    None
}

fn is_rfc822_header_line(line: &[u8]) -> bool {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let Some(colon) = line.iter().position(|byte| *byte == b':') else {
        return false;
    };
    if colon == 0 || !line[..colon].iter().all(|byte| byte.is_ascii_graphic()) {
        return false;
    }
    let name = &line[..colon];
    if !name
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        return false;
    }
    name.eq_ignore_ascii_case(b"from")
        || name.eq_ignore_ascii_case(b"to")
        || name.eq_ignore_ascii_case(b"cc")
        || name.eq_ignore_ascii_case(b"bcc")
        || name.eq_ignore_ascii_case(b"subject")
        || name.eq_ignore_ascii_case(b"date")
        || name.eq_ignore_ascii_case(b"received")
        || name.eq_ignore_ascii_case(b"message-id")
        || name.eq_ignore_ascii_case(b"content-type")
        || name.eq_ignore_ascii_case(b"mime-version")
        || name.eq_ignore_ascii_case(b"content-transfer-encoding")
        || name.eq_ignore_ascii_case(b"content-disposition")
        || name
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"x-"))
}
