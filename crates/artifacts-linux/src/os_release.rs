use crate::LinuxArtifactError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const OS_RELEASE_PATHS: [&str; 2] = ["etc/os-release", "usr/lib/os-release"];
pub const MAX_OS_RELEASE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OsReleaseInfo {
    pub pretty_name: Option<String>,
    pub id: Option<String>,
    pub version_id: Option<String>,
    pub fields: BTreeMap<String, String>,
}

pub fn parse_os_release(content: &str) -> Result<OsReleaseInfo, LinuxArtifactError> {
    if content.len() > MAX_OS_RELEASE_BYTES || content.contains('\0') {
        return Err(parse_error("invalid input size or NUL byte"));
    }
    let mut fields = BTreeMap::new();
    for (index, line) in content.lines().enumerate() {
        let line = line.trim();
        let decoded = match line.parse::<etc_os_release::OsReleaseLine<'static>>() {
            Ok(decoded) => decoded,
            Err(never) => match never {},
        };
        if let Some(entry) = decoded.into_entry() {
            if entry.key().is_empty() {
                return Err(parse_error("empty field name"));
            }
            // The POSIX lexer owns quote/escape handling and performs no variable expansion.
            let words = shlex::split(line).ok_or_else(|| parse_error("invalid field quoting"))?;
            let [assignment] = words.as_slice() else {
                return Err(parse_error("field is not one assignment"));
            };
            let value = assignment
                .strip_prefix(&format!("{}=", entry.key()))
                .ok_or_else(|| parse_error("invalid field assignment"))?;
            fields.insert(entry.key().to_owned(), value.to_owned());
        } else if !line.is_empty() && !line.starts_with('#') {
            return Err(parse_error(&format!("unrecognized line {}", index + 1)));
        }
    }
    if !fields.contains_key("ID") && !fields.contains_key("NAME") {
        return Err(parse_error("missing ID and NAME fields"));
    }
    Ok(OsReleaseInfo {
        pretty_name: fields.get("PRETTY_NAME").cloned(),
        id: fields.get("ID").cloned(),
        version_id: fields.get("VERSION_ID").cloned(),
        fields,
    })
}

pub fn parse_os_release_bytes(content: &[u8]) -> Result<OsReleaseInfo, LinuxArtifactError> {
    let content = std::str::from_utf8(content).map_err(|_| parse_error("invalid UTF-8"))?;
    parse_os_release(content)
}

fn parse_error(message: &str) -> LinuxArtifactError {
    LinuxArtifactError::ParseError {
        parser: "os-release",
        message: message.into(),
    }
}
