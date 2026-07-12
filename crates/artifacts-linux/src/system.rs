//! Linux system configuration parsers.
//!
//! These parsers intentionally avoid exposing sensitive credential material.
//! For example, `/etc/shadow` support should emit account status metadata only,
//! never password hash bytes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Account metadata from `/etc/passwd`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswdAccount {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub gecos: String,
    pub home: String,
    pub shell: String,
}

/// Operating-system metadata from `/etc/os-release` or `/usr/lib/os-release`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OsReleaseInfo {
    pub pretty_name: Option<String>,
    pub id: Option<String>,
    pub version_id: Option<String>,
    pub fields: BTreeMap<String, String>,
}

/// Parse `/etc/passwd` account records.
pub fn parse_passwd(content: &str) -> Result<Vec<PasswdAccount>, crate::LinuxArtifactError> {
    let mut accounts = Vec::new();

    for (line_number, line) in (1usize..).zip(content.lines()) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let fields = trimmed.splitn(7, ':').collect::<Vec<_>>();
        if fields.len() != 7 {
            return Err(crate::LinuxArtifactError::ParseError {
                parser: "passwd",
                message: format!("line {line_number} has {} fields, expected 7", fields.len()),
            });
        }

        let uid =
            fields[2]
                .parse::<u32>()
                .map_err(|err| crate::LinuxArtifactError::ParseError {
                    parser: "passwd",
                    message: format!("line {line_number} has invalid UID: {err}"),
                })?;
        let gid =
            fields[3]
                .parse::<u32>()
                .map_err(|err| crate::LinuxArtifactError::ParseError {
                    parser: "passwd",
                    message: format!("line {line_number} has invalid GID: {err}"),
                })?;

        accounts.push(PasswdAccount {
            username: fields[0].to_string(),
            uid,
            gid,
            gecos: fields[4].to_string(),
            home: fields[5].to_string(),
            shell: fields[6].to_string(),
        });
    }

    Ok(accounts)
}

/// Parse freedesktop `os-release` key/value data.
pub fn parse_os_release(content: &str) -> Result<OsReleaseInfo, crate::LinuxArtifactError> {
    let mut fields = BTreeMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        fields.insert(key.to_string(), unquote_os_release_value(raw_value.trim()));
    }

    Ok(OsReleaseInfo {
        pretty_name: fields.get("PRETTY_NAME").cloned(),
        id: fields.get("ID").cloned(),
        version_id: fields.get("VERSION_ID").cloned(),
        fields,
    })
}

fn unquote_os_release_value(value: &str) -> String {
    let quoted = (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''));
    if !quoted || value.len() < 2 {
        return value.to_string();
    }

    let inner = &value[1..value.len() - 1];
    let mut result = String::with_capacity(inner.len());
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            result.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            result.push(ch);
        }
    }
    if escaped {
        result.push('\\');
    }
    result
}

#[cfg(test)]
#[path = "../tests/unit/system.rs"]
mod tests;
