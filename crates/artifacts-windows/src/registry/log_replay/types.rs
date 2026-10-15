use super::header::Header;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSlot {
    Log1,
    Log2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Hvle,
    Dirt,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    NotNeeded,
    Recovered,
    Partial,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogDisposition {
    Applied,
    NotNeeded,
    Stale,
    Invalid,
    Unavailable,
    Duplicate,
    NotSelected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LogReplayError {
    #[error("registry base block is invalid")]
    Header,
    #[error("registry log format is unsupported")]
    Unsupported,
    #[error("registry log does not match the hive")]
    Identity,
    #[error("registry log checksum is invalid")]
    Checksum,
    #[error("registry log entry hash is invalid")]
    Hash,
    #[error("registry log is truncated")]
    Truncated,
    #[error("registry log sequence is discontinuous")]
    Sequence,
    #[error("registry log range exceeds the supported bounds")]
    Bounds,
    #[error("registry logs contain conflicting records")]
    Conflict,
    #[error("no applicable registry log is available")]
    NoLog,
    #[error("registry log content could not be read")]
    Unavailable,
    #[error("replayed registry hive is invalid")]
    InvalidHive,
}
impl LogReplayError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Header => "invalidBaseBlock",
            Self::Unsupported => "unsupportedLog",
            Self::Identity => "logIdentityMismatch",
            Self::Checksum => "logChecksumMismatch",
            Self::Hash => "logHashMismatch",
            Self::Truncated => "truncatedLog",
            Self::Sequence => "logSequenceGap",
            Self::Bounds => "logBoundsExceeded",
            Self::Conflict => "conflictingLogs",
            Self::NoLog => "noApplicableLog",
            Self::Unavailable => "logUnavailable",
            Self::InvalidHive => "invalidRecoveredHive",
        }
    }
}
pub struct HiveLog<'a> {
    pub slot: LogSlot,
    pub bytes: Option<&'a [u8]>,
}
#[derive(Debug, Clone)]
pub struct LogReplayReport {
    pub state: RecoveryState,
    pub base_primary: u32,
    pub base_secondary: u32,
    pub base_checksum_valid: bool,
    pub result_sequence: Option<u32>,
    pub applied_entries: u32,
    pub applied_pages: u32,
    pub logs: Vec<LogReport>,
    pub diagnostics: Vec<LogReplayError>,
}
#[derive(Debug, Clone)]
pub struct LogReport {
    pub slot: LogSlot,
    pub format: Option<LogFormat>,
    pub disposition: LogDisposition,
    pub first_sequence: Option<u32>,
    pub last_sequence: Option<u32>,
    pub applied_entries: u32,
    pub applied_pages: u32,
    pub diagnostic: Option<LogReplayError>,
}
#[derive(Debug, Error)]
#[error("registry recovery failed: {error}")]
pub struct LogReplayFailure {
    pub error: LogReplayError,
    pub report: LogReplayReport,
}
pub struct ReplayedHive {
    pub bytes: Vec<u8>,
    pub report: LogReplayReport,
    pub provenance: Vec<crate::registry::browser::ProvenanceSpan>,
}

pub struct LogReplayPlan<'a> {
    pub(super) base: Header,
    pub(super) logs: Vec<ParsedLog<'a>>,
    pub(super) order: Vec<usize>,
    pub(super) report: LogReplayReport,
    pub(super) max_bins: usize,
}
pub(super) struct ParsedLog<'a> {
    pub slot: LogSlot,
    pub data: &'a [u8],
    pub header: Header,
    pub format: LogFormat,
    pub records: Vec<Record>,
    pub tail: Option<LogReplayError>,
}
pub(super) struct Record {
    pub sequence: u32,
    pub bins: usize,
    pub flags: u32,
    pub pages: Vec<Page>,
    pub digest: u64,
}
pub(super) struct Page {
    pub offset: usize,
    pub data_start: usize,
    pub length: usize,
}

impl LogReplayPlan<'_> {
    pub fn required_capacity(&self, base_length: usize) -> usize {
        base_length.max(4096 + self.max_bins)
    }
    pub fn report(&self) -> &LogReplayReport {
        &self.report
    }
}
pub(super) fn failure(error: LogReplayError, mut report: LogReplayReport) -> LogReplayFailure {
    report.state = RecoveryState::Failed;
    report.diagnostics.push(error);
    LogReplayFailure { error, report }
}
