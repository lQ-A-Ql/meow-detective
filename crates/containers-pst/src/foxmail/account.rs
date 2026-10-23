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
    if encoded_hex.is_empty() || !encoded_hex.len().is_multiple_of(2) {
        return Err(FoxmailPasswordError::InvalidPasswordLength);
    }
    let mut encrypted = Vec::with_capacity(encoded_hex.len() / 2);
    for pair in encoded_hex.as_chunks::<2>().0 {
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
