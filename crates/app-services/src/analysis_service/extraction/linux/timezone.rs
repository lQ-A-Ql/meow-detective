//! Host timezone inference for naive Linux log timestamps.
//!
//! Classic syslog, auth.log, and the apt/dpkg/yum logs record local
//! wall-clock time without a zone. The host zone is resolved from, in order:
//!
//! 1. `/etc/localtime` — usually a symlink into the tz database; the evidence
//!    filesystem layer resolves the symlink and yields the target path text,
//!    which ends with `zoneinfo/<Area>/<City>`.
//! 2. `/etc/timezone` — Debian-family single-line zone name.
//! 3. `/etc/sysconfig/clock` — RHEL-family `ZONE="..."` assignment.
//!
//! The zone name is parsed with `chrono-tz` (full IANA database) rather than
//! reduced to a fixed offset: a fixed offset sampled at analysis time would
//! mis-convert log lines recorded on the far side of a DST transition, while
//! `chrono-tz` applies the historical DST rule for each date. Zones without
//! DST (e.g. Asia/Shanghai) behave exactly like their fixed offset.
//!
//! When no candidate yields a valid zone, UTC is assumed and a warning is
//! recorded ("timezone not determined, timestamps interpreted as UTC").

use super::super::reader::{read_candidate_source_with_progress, CandidateSource};
use crate::analysis_service::candidates::{find_candidate_by_path_suffix, EvidenceCandidate};
use artifacts_linux::LogClock;
use chrono::{DateTime, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use domain::FileEntry;
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

/// Cap on timezone hint file reads; the zone name is a few dozen bytes.
const TIMEZONE_READ_LIMIT: usize = 4096;

/// Warning recorded when no timezone source yields a valid zone.
pub(super) const UTC_FALLBACK_WARNING: &str =
    "timezone not determined, timestamps interpreted as UTC";

/// Resolved host timezone backing naive-to-UTC log timestamp conversion.
pub(in crate::analysis_service::extraction) struct LinuxLogTimeContext {
    zone: HostTimeZone,
}

impl LinuxLogTimeContext {
    /// UTC context used when timezone inference does not apply (non-Linux
    /// sources) or no zone could be determined.
    pub fn utc() -> Self {
        Self {
            zone: HostTimeZone {
                tz: Tz::UTC,
                assumed_utc: true,
            },
        }
    }

    fn for_zone(tz: Tz) -> Self {
        Self {
            zone: HostTimeZone {
                tz,
                assumed_utc: false,
            },
        }
    }

    /// Clock view passed to the artifacts-linux parsers.
    pub fn clock(&self) -> &dyn LogClock {
        &self.zone
    }

    /// Attr value recording the conversion basis: the zone name, or `utc`
    /// when the zone could not be determined.
    pub fn tz_label(&self) -> &str {
        if self.zone.assumed_utc {
            "utc"
        } else {
            self.zone.tz.name()
        }
    }

    /// Convert plugin timeline events that carry host wall-clock times
    /// (`timesAreLocal` payloads mark them with a `naiveLocalTime` attr) to
    /// UTC with the resolved host zone, removing the marker.
    pub fn convert_marked_local_events(&self, events: &mut [domain::TimelineEvent]) {
        for event in events {
            let Some(naive_text) = event.attrs.remove("naiveLocalTime") else {
                continue;
            };
            let Some(naive_text) = naive_text.as_str() else {
                continue;
            };
            let Ok(naive) =
                chrono::NaiveDateTime::parse_from_str(naive_text, "%Y-%m-%dT%H:%M:%S%.f")
            else {
                continue;
            };
            if let Some(converted) = self.zone.local_to_utc(naive) {
                event.timestamp = converted;
            }
        }
    }
}

/// `chrono-tz` backed clock. `Tz` is `Copy + Send + Sync`, so the context can
/// be shared with the parallel parse workers.
#[derive(Debug, Clone, Copy)]
struct HostTimeZone {
    tz: Tz,
    assumed_utc: bool,
}

impl LogClock for HostTimeZone {
    fn local_to_utc(&self, local: NaiveDateTime) -> Option<DateTime<Utc>> {
        match self.tz.from_local_datetime(&local) {
            LocalResult::Single(local_dt) => Some(local_dt.with_timezone(&Utc)),
            // Autumn fold: keep the earlier occurrence, per the LogClock docs.
            LocalResult::Ambiguous(earliest, _) => Some(earliest.with_timezone(&Utc)),
            // Spring-forward gap: the wall time never existed. Interpreting
            // it as UTC keeps a timestamp instead of dropping the event and
            // is off by at most the DST step.
            LocalResult::None => Some(Utc.from_utc_datetime(&local)),
        }
    }
    fn utc_to_local_naive(&self, timestamp: DateTime<Utc>) -> NaiveDateTime {
        timestamp.with_timezone(&self.tz).naive_local()
    }
}

/// Extracts a candidate zone name from a timezone hint file's text.
type ZoneExtractor = fn(&str) -> Option<String>;

/// Candidate timezone sources in priority order, each with the extractor
/// that pulls a zone name out of the file text.
const TIMEZONE_SOURCES: &[(&str, ZoneExtractor)] = &[
    ("/etc/localtime", zone_from_localtime),
    ("/etc/timezone", zone_from_timezone_file),
    ("/etc/sysconfig/clock", zone_from_sysconfig_clock),
];

/// Resolve the host timezone for Linux log parsing.
///
/// Returns the context plus any warnings to record against the Linux
/// capability (unreadable files, unrecognized zone names, UTC fallback).
pub(in crate::analysis_service::extraction) fn resolve_linux_log_time(
    conn: &Connection,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> (LinuxLogTimeContext, Vec<String>) {
    let mut warnings = Vec::new();
    for (path, extract) in TIMEZONE_SOURCES {
        match read_zone_text(conn, path, cancel_token, file_reader) {
            Ok(Some(text)) => {
                if let Some(name) = extract(&text) {
                    match name.parse::<Tz>() {
                        Ok(tz) => return (LinuxLogTimeContext::for_zone(tz), warnings),
                        Err(_) => warnings.push(format!(
                            "{path} names unknown timezone '{name}'; trying the next source"
                        )),
                    }
                }
            }
            Ok(None) => {}
            Err(warning) => warnings.push(warning),
        }
    }
    tracing::info!("{UTC_FALLBACK_WARNING}");
    warnings.push(UTC_FALLBACK_WARNING.to_string());
    (LinuxLogTimeContext::utc(), warnings)
}

/// Locate a timezone hint file and read its (small) content as text.
fn read_zone_text(
    conn: &Connection,
    path: &str,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<Option<String>, String> {
    let Some(entry) = find_candidate_by_path_suffix(conn, path)
        .map_err(|error| format!("{path} lookup failed: {error}"))?
    else {
        return Ok(None);
    };
    if entry.encrypted {
        return Ok(None);
    }
    let candidate = timezone_probe_candidate(&entry);
    let source = file_reader(&candidate, TIMEZONE_READ_LIMIT)
        .map_err(|error| format!("{path} read failed: {error}"))?;
    let bytes = read_candidate_source_with_progress(
        &candidate,
        source,
        TIMEZONE_READ_LIMIT,
        cancel_token,
        |_| {},
    )
    .map_err(|error| format!("{path} read failed: {error:?}"))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Minimal candidate view used only to route a timezone file through the
/// standard bounded reader.
fn timezone_probe_candidate(entry: &FileEntry) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: entry.id.clone(),
        data_source_id: entry.data_source_id.0.clone(),
        partition_index: None,
        path: entry.path.clone(),
        size: entry.size.unwrap_or(0),
        encrypted: entry.encrypted,
        content_identity: String::new(),
        companions: Vec::new(),
        evidence_kind: "linux_timezone".to_string(),
        parser: "linux.timezone".to_string(),
        category: String::new(),
        modified_at: entry.modified_at,
    }
}

/// `/etc/localtime` symlink target text ends with `zoneinfo/<Area>/<City>`.
/// A copied (non-symlink) TZif binary yields no zone name here.
fn zone_from_localtime(text: &str) -> Option<String> {
    let text = text.trim();
    let start = text.find("zoneinfo/")? + "zoneinfo/".len();
    let name = text[start..]
        .split(|c: char| c.is_whitespace() || c == '\0')
        .next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// `/etc/timezone` holds the zone name on a single line (Debian family).
fn zone_from_timezone_file(text: &str) -> Option<String> {
    let name = text.lines().next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// `/etc/sysconfig/clock` holds `ZONE="<Area>/<City>"` (RHEL family).
fn zone_from_sysconfig_clock(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(value) = line.trim().strip_prefix("ZONE=") {
            let name = value.trim().trim_matches('"').trim_matches('\'').trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/linux/timezone.rs"]
mod tests;
