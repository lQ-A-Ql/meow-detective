use std::fmt;

use domain::DataSourceId;

use crate::connection::{DbError, DbResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessingPhase {
    Catalog,
    Graph,
    Platform,
    Artifacts,
    Timeline,
    Search,
}

impl ProcessingPhase {
    pub const ALL: [Self; 6] = [
        Self::Catalog,
        Self::Graph,
        Self::Platform,
        Self::Artifacts,
        Self::Timeline,
        Self::Search,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Graph => "graph",
            Self::Platform => "platform",
            Self::Artifacts => "artifacts",
            Self::Timeline => "timeline",
            Self::Search => "search",
        }
    }

    pub(super) fn from_storage(value: &str) -> DbResult<Self> {
        match value {
            "catalog" => Ok(Self::Catalog),
            "graph" => Ok(Self::Graph),
            "platform" => Ok(Self::Platform),
            "artifacts" => Ok(Self::Artifacts),
            "timeline" => Ok(Self::Timeline),
            "search" => Ok(Self::Search),
            _ => Err(DbError::System(format!(
                "stored processing phase is invalid: {value}"
            ))),
        }
    }
}

impl fmt::Display for ProcessingPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessingPhaseState {
    Pending,
    Running,
    Ready,
    Failed,
    Deferred,
}

impl ProcessingPhaseState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Deferred => "deferred",
        }
    }

    pub(super) fn from_storage(value: &str) -> DbResult<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            "deferred" => Ok(Self::Deferred),
            _ => Err(DbError::System(format!(
                "stored processing phase state is invalid: {value}"
            ))),
        }
    }
}

impl fmt::Display for ProcessingPhaseState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourceProcessingPhaseRecord {
    pub data_source_id: DataSourceId,
    pub phase: ProcessingPhase,
    pub state: ProcessingPhaseState,
    pub version: u32,
    pub input_fingerprint: String,
    pub owner_id: Option<String>,
    pub attempt_id: Option<String>,
    pub stats_json: String,
    pub last_error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub lease_expires_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessingPhaseClaim {
    Acquired(DataSourceProcessingPhaseRecord),
    Ready(DataSourceProcessingPhaseRecord),
    Busy(DataSourceProcessingPhaseRecord),
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessingPhaseTransition<'a> {
    pub state: ProcessingPhaseState,
    pub stats_json: &'a str,
    pub last_error: Option<&'a str>,
}

impl<'a> ProcessingPhaseTransition<'a> {
    pub const fn ready(stats_json: &'a str) -> Self {
        Self {
            state: ProcessingPhaseState::Ready,
            stats_json,
            last_error: None,
        }
    }

    pub const fn failed(stats_json: &'a str, last_error: &'a str) -> Self {
        Self {
            state: ProcessingPhaseState::Failed,
            stats_json,
            last_error: Some(last_error),
        }
    }

    pub const fn deferred(stats_json: &'a str, reason: Option<&'a str>) -> Self {
        Self {
            state: ProcessingPhaseState::Deferred,
            stats_json,
            last_error: reason,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessingPhaseCompletion<'a> {
    pub version: u32,
    pub input_fingerprint: &'a str,
    pub owner_id: &'a str,
    pub attempt_id: &'a str,
    pub transition: ProcessingPhaseTransition<'a>,
}

impl<'a> ProcessingPhaseCompletion<'a> {
    pub const fn new(
        version: u32,
        input_fingerprint: &'a str,
        owner_id: &'a str,
        attempt_id: &'a str,
        transition: ProcessingPhaseTransition<'a>,
    ) -> Self {
        Self {
            version,
            input_fingerprint,
            owner_id,
            attempt_id,
            transition,
        }
    }
}
