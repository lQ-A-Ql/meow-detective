//! Linux system configuration parsers.
//!
//! These parsers intentionally avoid exposing sensitive credential material.
//! For example, `/etc/shadow` support should emit account status metadata only,
//! never password hash bytes.

use serde::{Deserialize, Serialize};

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

#[cfg(test)]
#[path = "../tests/unit/system.rs"]
mod tests;
