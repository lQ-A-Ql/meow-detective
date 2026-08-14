//! `/etc/shadow` parsing and password-hash replacement for offline bypass.
//!
//! The caller supplies a complete crypt-format password hash and remains
//! responsible for the rewrite-and-truncate write-back.

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
    let mut lines = content.split('\n');
    let mut output = String::with_capacity(content.len());
    let mut edited = false;
    let mut first = true;
    for line in lines.by_ref() {
        if !first {
            output.push('\n');
        }
        first = false;
        let mut fields = line.splitn(3, ':');
        let (Some(name), Some(hash), Some(rest)) = (fields.next(), fields.next(), fields.next())
        else {
            output.push_str(line);
            continue;
        };
        if name != username || hash == password_hash {
            output.push_str(line);
            continue;
        }
        output.push_str(name);
        output.push(':');
        output.push_str(password_hash);
        output.push(':');
        output.push_str(rest);
        edited = true;
    }
    Ok(edited.then_some(output))
}

#[cfg(test)]
#[path = "../tests/unit/shadow_edit.rs"]
mod tests;
