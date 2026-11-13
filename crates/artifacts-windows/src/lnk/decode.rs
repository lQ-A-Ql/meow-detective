use chrono::{DateTime, TimeZone, Utc};
use std::io::Read;

const MAX_LINK_BYTES: u64 = 64 * 1024 * 1024;
const LINK_HEADER_BYTES: usize = 76;

pub(crate) struct DecodedLnk {
    pub(crate) file_size: u32,
    pub(crate) target_path: String,
    pub(crate) timestamps: Vec<LnkTimestamp>,
}

pub(crate) struct LnkTimestamp {
    pub(crate) field: &'static str,
    pub(crate) event_type: &'static str,
    pub(crate) value: DateTime<Utc>,
}

pub(super) fn decode_lnk(reader: &mut impl Read) -> Result<DecodedLnk, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_LINK_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_LINK_BYTES {
        return Err("Shell Link exceeds the supported input limit".into());
    }
    if bytes.len() < LINK_HEADER_BYTES {
        return Err("Shell Link header is truncated".into());
    }
    let link = shell::lnk::parse(&bytes).map_err(|error| error.to_string())?;
    Ok(project_link(&link))
}

pub(crate) fn project_link(link: &shell::lnk::Link) -> DecodedLnk {
    DecodedLnk {
        file_size: link.size,
        target_path: target_path(link),
        timestamps: [
            ("creation_time", "LINK_CREATED", link.created),
            ("access_time", "LINK_ACCESSED", link.accessed),
            ("write_time", "LINK_MODIFIED", link.modified),
        ]
        .into_iter()
        .filter_map(|(field, event_type, ticks)| {
            filetime_to_datetime(ticks).map(|value| LnkTimestamp {
                field,
                event_type,
                value,
            })
        })
        .collect(),
    }
}

fn target_path(link: &shell::lnk::Link) -> String {
    if !link.local_path.is_empty() {
        return join_target(&link.local_path, &link.common_path);
    }
    if !link.network_path.is_empty() {
        return join_target(&link.network_path, &link.common_path);
    }
    let shell_path = link.target_path();
    if !shell_path.is_empty() {
        return shell_path;
    }
    if !link.environment_target.is_empty() {
        return link.environment_target.clone();
    }
    link.relative_path.clone()
}

fn join_target(base: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        return base.to_string();
    }
    format!(
        "{}\\{}",
        base.trim_end_matches('\\'),
        suffix.trim_start_matches('\\')
    )
}

fn filetime_to_datetime(value: u64) -> Option<DateTime<Utc>> {
    if value == 0 {
        return None;
    }
    let seconds = i64::try_from(value / 10_000_000)
        .ok()?
        .checked_sub(11_644_473_600)?;
    Utc.timestamp_opt(seconds, ((value % 10_000_000) * 100) as u32)
        .single()
}
