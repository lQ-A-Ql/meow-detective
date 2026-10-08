//! Bounded parser for systemd unit files.
//!
//! The parser intentionally consumes unit text only.  Runtime state is not
//! available in an offline image, therefore `enabled` and `state` are
//! inferred from unit placement (`*.wants`/`*.requires`) and install
//! metadata.  All limits are explicit so a malformed unit cannot allocate
//! unbounded memory during evidence import.

use crate::LinuxArtifactError;

const MAX_UNIT_BYTES: usize = 1024 * 1024;
const MAX_UNIT_LINES: usize = 4096;
const MAX_VALUE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxServiceUnit {
    pub name: String,
    pub description: Option<String>,
    pub state: String,
    pub enabled: bool,
    pub masked: bool,
    pub static_unit: bool,
    pub unit_file: String,
    pub exec_start: Vec<String>,
    pub exec_stop: Vec<String>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub working_directory: Option<String>,
    pub restart: Option<String>,
    pub wanted_by: Vec<String>,
    pub required_by: Vec<String>,
    pub enablement_symlink: Option<String>,
}

/// Parse one systemd unit.  `unit_file` is the evidence path (without
/// normalization assumptions) and is retained for provenance and enablement
/// inference.  Values are bounded and duplicate scalar keys use the last
/// value, matching systemd's usual override behaviour.
pub fn parse_systemd_unit(
    unit_file: &str,
    bytes: &[u8],
) -> Result<LinuxServiceUnit, LinuxArtifactError> {
    if bytes.len() > MAX_UNIT_BYTES {
        return Err(LinuxArtifactError::ParseError {
            parser: "systemd.unit",
            message: "unit file exceeds 1 MiB limit".to_string(),
        });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| LinuxArtifactError::ParseError {
        parser: "systemd.unit",
        message: "unit file is not UTF-8".to_string(),
    })?;
    let name = unit_file
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(unit_file)
        .to_string();
    let masked = text.trim() == "/dev/null";
    let mut section = String::new();
    let mut description = None;
    let mut exec_start = Vec::new();
    let mut exec_stop = Vec::new();
    let mut user = None;
    let mut group = None;
    let mut working_directory = None;
    let mut restart = None;
    let mut wanted_by = Vec::new();
    let mut required_by = Vec::new();
    let mut has_install_section = false;
    let mut line_count = 0usize;

    for raw_line in text.lines() {
        line_count += 1;
        if line_count > MAX_UNIT_LINES {
            return Err(LinuxArtifactError::ParseError {
                parser: "systemd.unit",
                message: "unit file exceeds line limit".to_string(),
            });
        }
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(section_name) = line.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            section.clear();
            section.push_str(section_name.trim());
            has_install_section = section == "Install" || has_install_section;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.len() > MAX_VALUE_BYTES {
            continue;
        }
        match (section.as_str(), key.trim()) {
            ("Unit", "Description") => description = non_empty(value),
            ("Service", "ExecStart") => push_value(&mut exec_start, value),
            ("Service", "ExecStop") => push_value(&mut exec_stop, value),
            ("Service", "User") => user = non_empty(value),
            ("Service", "Group") => group = non_empty(value),
            ("Service", "WorkingDirectory") => working_directory = non_empty(value),
            ("Service", "Restart") => restart = non_empty(value),
            ("Install", "WantedBy") => extend_words(&mut wanted_by, value),
            ("Install", "RequiredBy") => extend_words(&mut required_by, value),
            _ => {}
        }
    }

    let enablement_symlink = infer_enablement_symlink(unit_file);
    // `[Install]` describes how a unit *can* be enabled; only a concrete
    // `.wants/` or `.requires/` evidence path proves it is enabled in the
    // captured image.
    let enabled = enablement_symlink.is_some();
    let static_unit = !masked && !has_install_section;
    let state = if masked {
        "masked"
    } else if enabled {
        "enabled"
    } else if static_unit {
        "static"
    } else {
        "disabled"
    }
    .to_string();

    Ok(LinuxServiceUnit {
        name,
        description,
        state,
        enabled,
        masked,
        static_unit,
        unit_file: unit_file.to_string(),
        exec_start,
        exec_stop,
        user,
        group,
        working_directory,
        restart,
        wanted_by,
        required_by,
        enablement_symlink,
    })
}

fn push_value(values: &mut Vec<String>, value: &str) {
    if !value.is_empty() && values.len() < 64 {
        values.push(value.to_string());
    }
}

fn extend_words(values: &mut Vec<String>, value: &str) {
    for item in value
        .split_whitespace()
        .take(64usize.saturating_sub(values.len()))
    {
        if !item.is_empty() {
            values.push(item.to_string());
        }
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn infer_enablement_symlink(path: &str) -> Option<String> {
    (path.contains(".wants/") || path.contains(".requires/")).then(|| path.to_string())
}
