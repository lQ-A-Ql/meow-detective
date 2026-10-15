use super::{MAX_UNIT_LINES, MAX_VALUE_BYTES};
use crate::LinuxArtifactError;
pub(super) fn logical_lines(text: &str) -> Result<Vec<String>, LinuxArtifactError> {
    let mut lines = Vec::new();
    let mut pending = String::new();
    for (index, raw) in text.lines().enumerate() {
        if index >= MAX_UNIT_LINES {
            return Err(error("unit file exceeds line limit"));
        }
        let line = raw.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let continued = line.ends_with('\\');
        let value = if continued {
            &line[..line.len() - 1]
        } else {
            line
        };
        if !pending.is_empty() {
            pending.push(' ');
        }
        pending.push_str(value);
        if pending.len() > MAX_VALUE_BYTES {
            return Err(error("logical line exceeds value limit"));
        }
        if !continued {
            lines.push(std::mem::take(&mut pending));
        }
    }
    if !pending.is_empty() {
        return Err(error("unterminated line continuation"));
    }
    Ok(lines)
}
pub(super) fn error(message: &str) -> LinuxArtifactError {
    LinuxArtifactError::ParseError {
        parser: "systemd.unit",
        message: message.into(),
    }
}
