//! `/etc/shadow` parsing and password-hash replacement for offline bypass.
//!
//! The caller supplies a complete crypt-format password hash and remains
//! responsible for bounded filesystem write-back.

use crate::LinuxArtifactError;

/// Account password state from one `/etc/shadow` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowAccount {
    pub username: String,
    /// The hash field holds a real password hash (not empty, `!` or `*`).
    pub has_password: bool,
    /// The hash field starts with `!` (locked account).
    pub locked: bool,
}

/// Parse account states from `/etc/shadow` content. Malformed lines (no
/// field separator) are skipped.
pub fn parse_shadow_accounts(content: &str) -> Vec<ShadowAccount> {
    content
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(':');
            let username = fields.next()?;
            if username.is_empty() {
                return None;
            }
            let hash = fields.next().unwrap_or("");
            Some(ShadowAccount {
                username: username.to_string(),
                has_password: !hash.is_empty() && hash != "*" && !hash.starts_with('!'),
                locked: hash.starts_with('!'),
            })
        })
        .collect()
}

/// Return `/etc/shadow` content with `username`'s hash field replaced, or
/// `None` when the account is absent or already uses the requested hash.
/// Every other byte (including the trailing-newline convention) is preserved.
pub fn set_shadow_password_hash(
    content: &str,
    username: &str,
    password_hash: &str,
) -> Result<Option<String>, LinuxArtifactError> {
    edit_shadow_account(content, username, password_hash, None)
}

/// Replace one password hash and make its shadow aging fields usable now.
///
/// The last-change day is advanced to `current_day`. An absolute account
/// expiration that has already elapsed is cleared; future expiration and all
/// other policy fields are preserved. This is intended for an explicit,
/// caller-controlled emulation overlay, never for an evidence source.
pub fn set_shadow_login_password(
    content: &str,
    username: &str,
    password_hash: &str,
    current_day: u64,
) -> Result<Option<String>, LinuxArtifactError> {
    edit_shadow_account(content, username, password_hash, Some(current_day))
}

fn edit_shadow_account(
    content: &str,
    username: &str,
    password_hash: &str,
    current_day: Option<u64>,
) -> Result<Option<String>, LinuxArtifactError> {
    validate_edit_inputs(username, password_hash)?;
    let mut output = String::with_capacity(content.len());
    let mut edited = false;
    for (index, line) in content.split('\n').enumerate() {
        if index > 0 {
            output.push('\n');
        }
        let Some(replacement) = edit_target_line(line, username, password_hash, current_day)?
        else {
            output.push_str(line);
            continue;
        };
        edited |= replacement != line;
        output.push_str(&replacement);
    }
    Ok(edited.then_some(output))
}

fn validate_edit_inputs(username: &str, password_hash: &str) -> Result<(), LinuxArtifactError> {
    if username.is_empty() || username.contains(':') || username.contains('\n') {
        return Err(LinuxArtifactError::ParseError {
            parser: "shadow",
            message: "invalid username for a shadow edit".to_string(),
        });
    }
    if password_hash.is_empty()
        || password_hash.contains(':')
        || password_hash.contains('\n')
        || password_hash.starts_with('!')
        || password_hash.starts_with('*')
    {
        return Err(LinuxArtifactError::ParseError {
            parser: "shadow",
            message: "invalid replacement password hash".to_string(),
        });
    }
    Ok(())
}

fn edit_target_line(
    line: &str,
    username: &str,
    password_hash: &str,
    current_day: Option<u64>,
) -> Result<Option<String>, LinuxArtifactError> {
    if line.split_once(':').map(|(name, _)| name) != Some(username) {
        return Ok(None);
    }
    let mut fields = line.split(':').collect::<Vec<_>>();
    let required_fields = if current_day.is_some() { 8 } else { 3 };
    if fields.len() < required_fields {
        return Err(LinuxArtifactError::ParseError {
            parser: "shadow",
            message: "target account has malformed shadow fields".to_string(),
        });
    }
    fields[1] = password_hash;
    if let Some(day) = current_day {
        let day_text = day.to_string();
        fields[2] = &day_text;
        if shadow_expiration_elapsed(fields[7], day)? {
            fields[7] = "";
        }
        return Ok(Some(fields.join(":")));
    }
    Ok(Some(fields.join(":")))
}

fn shadow_expiration_elapsed(value: &str, current_day: u64) -> Result<bool, LinuxArtifactError> {
    if value.is_empty() || value == "0" {
        return Ok(false);
    }
    value
        .parse::<u64>()
        .map(|day| day <= current_day)
        .map_err(|_| LinuxArtifactError::ParseError {
            parser: "shadow",
            message: "target account has an invalid expiration day".to_string(),
        })
}

#[cfg(test)]
#[path = "../tests/unit/shadow_edit.rs"]
mod tests;
