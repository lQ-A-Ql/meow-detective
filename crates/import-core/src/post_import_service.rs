//! Scoped post-import use cases, execution ownership and event publication.

use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRow;
use source_kernel::task_runtime::{TaskManager, TaskRegistrationError};
use std::{path::PathBuf, sync::Arc};
use transport::events::EventEnvelope;

mod artifacts;
mod enqueue;
mod events;
mod execution;
mod progress;
mod wait;

pub use enqueue::{cancel_capability, enqueue_case_pending, retry_capability};
pub use events::emit as publish_application_event;
pub use wait::request_and_wait;

pub const CAPABILITY_TASK_GROUP_PREFIX: &str = "post-import|";

pub trait ApplicationEventSink: Send + Sync {
    fn emit(&self, event: &EventEnvelope<serde_json::Value>) -> std::io::Result<()>;
}

#[derive(Clone)]
pub struct PostImportRuntime {
    pub tasks: Arc<TaskManager>,
    pub bitlocker: Arc<evidence_access::bitlocker_runtime::BitLockerUnlockRegistry>,
    pub events: Option<Arc<dyn ApplicationEventSink>>,
    pub timeline: Arc<dyn TimelineProjectionExecutor>,
    pub case_projections: Arc<dyn CaseProjectionPublisher>,
}

pub trait CaseProjectionPublisher: Send + Sync {
    fn request(&self, case_id: &str, case_root: &std::path::Path) -> std::io::Result<()>;
}

pub struct TimelineProjectionRequest<'a> {
    pub conn: &'a rusqlite::Connection,
    pub case_root: &'a std::path::Path,
    pub case_id: &'a domain::CaseId,
    pub source_id: &'a str,
    pub cancel: &'a std::sync::atomic::AtomicBool,
}

pub trait TimelineProjectionExecutor: Send + Sync {
    fn project(&self, request: TimelineProjectionRequest<'_>) -> CapabilityOutcome;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutableCapability {
    Timeline,
    Artifacts,
}

impl ExecutableCapability {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Timeline => "timeline",
            Self::Artifacts => "artifacts",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "timeline" => Some(Self::Timeline),
            "artifacts" => Some(Self::Artifacts),
            _ => None,
        }
    }
}

pub enum RetryOutcome {
    Accepted,
    AlreadyActive,
    Full,
}

#[derive(Debug, thiserror::Error)]
pub enum PostImportError {
    #[error(transparent)]
    Database(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Storage(#[from] source_kernel::storage_runtime::StorageRuntimeError),
    #[error(transparent)]
    Event(#[from] std::io::Error),
    #[error(transparent)]
    Registration(#[from] TaskRegistrationError),
    #[error("post-import work cancelled")]
    Cancelled,
    #[error("capability execution is already active")]
    AlreadyActive,
    #[error("capability retry queue is full")]
    QueueFull,
    #[error("capability execution failed")]
    ExecutionFailed,
    #[error("capability execution was interrupted")]
    Interrupted,
}

impl transport::ServiceErrorCategory for PostImportError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::AlreadyActive | Self::QueueFull => transport::ErrorCategory::Validation,
            Self::Database(_) | Self::Storage(_) | Self::Event(_) => transport::ErrorCategory::Io,
            Self::Registration(_) | Self::ExecutionFailed | Self::Interrupted => {
                transport::ErrorCategory::Internal
            }
        }
    }

    fn code(&self) -> Option<&'static str> {
        match self {
            Self::AlreadyActive => Some("CAPABILITY_EXECUTION_IN_PROGRESS"),
            Self::QueueFull => Some("CAPABILITY_RETRY_QUEUE_FULL"),
            Self::ExecutionFailed => Some("CAPABILITY_EXECUTION_FAILED"),
            Self::Interrupted => Some("CAPABILITY_EXECUTION_INTERRUPTED"),
            _ => None,
        }
    }
}

struct CapabilityWork {
    row: SourceCapabilityRow,
    case_root: PathBuf,
    kind: ExecutableCapability,
    bitlocker: Arc<evidence_access::bitlocker_runtime::BitLockerUnlockRegistry>,
    events: Option<Arc<dyn ApplicationEventSink>>,
    timeline: Arc<dyn TimelineProjectionExecutor>,
    case_projections: Arc<dyn CaseProjectionPublisher>,
    progress: progress::CapabilityProgress,
}

pub struct CapabilityOutcome {
    pub state: &'static str,
    pub detail: Option<String>,
    pub error_code: Option<&'static str>,
    pub timeline_events: u64,
}

impl CapabilityOutcome {
    pub fn terminal(state: &'static str) -> Self {
        Self {
            state,
            detail: None,
            error_code: None,
            timeline_events: 0,
        }
    }
    pub fn failed(code: &'static str) -> Self {
        Self {
            error_code: Some(code),
            ..Self::terminal("failed")
        }
    }
}
