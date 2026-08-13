//! Local-clock abstraction for naive log timestamps.
//!
//! Several Linux text logs (classic syslog, auth.log, apt/dpkg/yum logs)
//! record wall-clock time without a timezone. Converting those timestamps to
//! UTC requires the host's local zone, which lives in an IANA tz database
//! this crate deliberately does not depend on: callers inject a [`LogClock`]
//! implementation instead. The application layer backs it with `chrono-tz`
//! so DST history is honored — a fixed offset sampled today would mis-convert
//! log lines from the far side of a DST transition.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

/// Converts between a host's local wall clock and UTC.
pub trait LogClock: Send + Sync {
    /// Convert a naive local timestamp to UTC.
    ///
    /// Returns `None` when the local time can never have existed (a DST
    /// spring-forward gap) and the implementation chooses not to approximate.
    /// Ambiguous local times (autumn fold) resolve to the earlier occurrence.
    fn local_to_utc(&self, local: NaiveDateTime) -> Option<DateTime<Utc>>;
    /// Render a UTC timestamp in the host's local wall clock.
    fn utc_to_local_naive(&self, timestamp: DateTime<Utc>) -> NaiveDateTime;
}

/// Identity clock: naive timestamps are interpreted as UTC.
#[derive(Debug, Clone, Copy, Default)]
pub struct UtcClock;

impl LogClock for UtcClock {
    fn local_to_utc(&self, local: NaiveDateTime) -> Option<DateTime<Utc>> {
        Some(Utc.from_utc_datetime(&local))
    }
    fn utc_to_local_naive(&self, timestamp: DateTime<Utc>) -> NaiveDateTime {
        timestamp.naive_utc()
    }
}

/// Year anchor and local clock for logs whose timestamps lack a year, a
/// zone, or both.
pub struct LogTimeHint<'a> {
    /// Reference instant (typically the log file's mtime) used to anchor
    /// year-less syslog timestamps. `None` falls back to the current time.
    pub reference: Option<DateTime<Utc>>,
    /// Host local clock used to convert naive timestamps to UTC.
    pub clock: &'a dyn LogClock,
}

impl LogTimeHint<'_> {
    /// Hint that interprets naive timestamps as UTC.
    pub fn utc(reference: Option<DateTime<Utc>>) -> Self {
        static UTC_CLOCK: UtcClock = UtcClock;
        Self {
            reference,
            clock: &UTC_CLOCK,
        }
    }

    /// Reference instant, defaulting to now when the caller has no anchor.
    pub fn reference_or_now(&self) -> DateTime<Utc> {
        self.reference.unwrap_or_else(Utc::now)
    }
}
