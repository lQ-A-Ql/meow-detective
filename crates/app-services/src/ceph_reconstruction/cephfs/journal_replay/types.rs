use thiserror::Error;

use super::super::{CephFsInventoryError, CephFsObjectReadError, CephFsObjectReadProvenance};

pub const DEFAULT_MAX_JOURNAL_EVENTS: usize = 250_000;
pub const DEFAULT_MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_MAX_JOURNAL_SOURCE_SPANS: usize = 500_000;
pub const DEFAULT_MAX_JOURNAL_PROVENANCE_ENTRIES: usize = 1_000_000;
pub const DEFAULT_MAX_JOURNAL_RETAINED_BYTES: u64 = 128 * 1024 * 1024;
pub const HARD_MAX_JOURNAL_EVENTS: usize = 1_000_000;
pub const HARD_MAX_JOURNAL_BYTES: u64 = 256 * 1024 * 1024;
pub const HARD_MAX_JOURNAL_SOURCE_SPANS: usize = 1_000_000;
pub const HARD_MAX_JOURNAL_PROVENANCE_ENTRIES: usize = 2_000_000;
pub const HARD_MAX_JOURNAL_RETAINED_BYTES: u64 = 224 * 1024 * 1024;
const MAX_CONTROL_OBJECT_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephFsJournalReplayLimits {
    pub max_events: usize,
    pub max_bytes: u64,
    pub max_event_bytes: usize,
    pub max_source_spans: usize,
    pub max_provenance_entries: usize,
    pub max_retained_bytes: u64,
}

impl Default for CephFsJournalReplayLimits {
    fn default() -> Self {
        Self {
            max_events: DEFAULT_MAX_JOURNAL_EVENTS,
            max_bytes: DEFAULT_MAX_JOURNAL_BYTES,
            max_event_bytes: ceph_wire::CEPHFS_JOURNAL_MAX_EVENT_BYTES,
            max_source_spans: DEFAULT_MAX_JOURNAL_SOURCE_SPANS,
            max_provenance_entries: DEFAULT_MAX_JOURNAL_PROVENANCE_ENTRIES,
            max_retained_bytes: DEFAULT_MAX_JOURNAL_RETAINED_BYTES,
        }
    }
}

impl CephFsJournalReplayLimits {
    pub(super) fn validate(self) -> Result<Self, CephFsJournalReplayError> {
        if self.max_events == 0
            || self.max_events > HARD_MAX_JOURNAL_EVENTS
            || self.max_bytes == 0
            || self.max_bytes > HARD_MAX_JOURNAL_BYTES
            || self.max_event_bytes == 0
            || self.max_event_bytes > ceph_wire::CEPHFS_JOURNAL_MAX_EVENT_BYTES
            || self.max_source_spans < 2
            || self.max_source_spans > HARD_MAX_JOURNAL_SOURCE_SPANS
            || self.max_provenance_entries < 2
            || self.max_provenance_entries > HARD_MAX_JOURNAL_PROVENANCE_ENTRIES
            || self.max_retained_bytes == 0
            || self.max_retained_bytes > HARD_MAX_JOURNAL_RETAINED_BYTES
        {
            return Err(CephFsJournalReplayError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalFramingStatus {
    Clean,
    CompleteToHeaderTail,
    Incomplete,
}

impl CephFsJournalFramingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::CompleteToHeaderTail => "complete_to_header_tail",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalStopReason {
    ByteBudget,
    EventBudget,
    TruncatedFrame,
    ObjectUnavailable,
    ReplicaConflict,
    ResponseMismatch,
    InvalidFrame,
    SourceSpanBudget,
    ProvenanceBudget,
    RetainedMemoryBudget,
}

impl CephFsJournalStopReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByteBudget => "byte_budget",
            Self::EventBudget => "event_budget",
            Self::TruncatedFrame => "truncated_frame",
            Self::ObjectUnavailable => "object_unavailable",
            Self::ReplicaConflict => "replica_conflict",
            Self::ResponseMismatch => "response_mismatch",
            Self::InvalidFrame => "invalid_frame",
            Self::SourceSpanBudget => "source_span_budget",
            Self::ProvenanceBudget => "provenance_budget",
            Self::RetainedMemoryBudget => "retained_memory_budget",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalNamespaceStopReason {
    BackupJournalPresent,
    NoMajorBoundary,
    LegacyEventEncoding,
    UnknownEvent,
    MutationPayloadUnsupported,
    SequenceConflict,
    SequenceSemanticsUnsupported,
    FramingIncomplete,
}

impl CephFsJournalNamespaceStopReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BackupJournalPresent => "backup_journal_present",
            Self::NoMajorBoundary => "no_major_boundary",
            Self::LegacyEventEncoding => "legacy_event_encoding",
            Self::UnknownEvent => "unknown_event",
            Self::MutationPayloadUnsupported => "mutation_payload_unsupported",
            Self::SequenceConflict => "sequence_conflict",
            Self::SequenceSemanticsUnsupported => "sequence_semantics_unsupported",
            Self::FramingIncomplete => "framing_incomplete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalSequenceStatus {
    Validated,
    IgnoredNonInitialLid,
    Frozen,
}

impl CephFsJournalSequenceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validated => "validated",
            Self::IgnoredNonInitialLid => "ignored_non_initial_lid",
            Self::Frozen => "frozen",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalSequenceStopReason {
    Conflict,
    UnknownEvent,
    UnsupportedSemantics,
    Overflow,
}

impl CephFsJournalSequenceStopReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conflict => "conflict",
            Self::UnknownEvent => "unknown_event",
            Self::UnsupportedSemantics => "unsupported_semantics",
            Self::Overflow => "overflow",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalSourceSpan {
    pub locator: String,
    pub logical_offset: u64,
    pub object_offset: u64,
    pub length: u64,
    pub range_sha256: String,
    pub provenance: Vec<CephFsObjectReadProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalReplayEvent {
    pub ordinal: u64,
    pub rank_local_segment_sequence: Option<u64>,
    pub rank_local_event_sequence: u64,
    pub sequence_status: CephFsJournalSequenceStatus,
    pub frame: ceph_wire::CephFsJournalFrame,
    pub spans: Vec<CephFsJournalSourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalReplay {
    pub filesystem_identity: String,
    pub fsmap_epoch: u32,
    pub mdsmap_epoch: u32,
    pub rank: u32,
    pub rank_gid: u64,
    pub rank_incarnation: i32,
    pub pointer: ceph_wire::CephFsJournalPointer,
    pub header: ceph_wire::CephFsJournalHeader,
    pub committed_header_tail: u64,
    pub framing_safe_pos: u64,
    pub namespace_safe_pos: Option<u64>,
    pub sequence_safe_pos: Option<u64>,
    pub framing_status: CephFsJournalFramingStatus,
    pub stop_reason: Option<CephFsJournalStopReason>,
    pub namespace_stop_reason: Option<CephFsJournalNamespaceStopReason>,
    pub sequence_stop_reason: Option<CephFsJournalSequenceStopReason>,
    pub pointer_spans: Vec<CephFsJournalSourceSpan>,
    pub header_spans: Vec<CephFsJournalSourceSpan>,
    pub events: Vec<CephFsJournalReplayEvent>,
    pub replay_sha256: String,
}

pub(super) struct RetainedBudget {
    source_spans: usize,
    provenance_entries: usize,
    retained_bytes: u64,
}

impl RetainedBudget {
    pub(super) fn new(
        pointer_spans: &[CephFsJournalSourceSpan],
        header_spans: &[CephFsJournalSourceSpan],
    ) -> Self {
        let mut budget = Self {
            source_spans: 0,
            provenance_entries: 0,
            retained_bytes: 0,
        };
        for span in pointer_spans.iter().chain(header_spans) {
            budget.source_spans = budget.source_spans.saturating_add(1);
            budget.provenance_entries = budget
                .provenance_entries
                .saturating_add(span.provenance.len());
            budget.retained_bytes = budget
                .retained_bytes
                .saturating_add(retained_span_bytes(span));
        }
        budget
    }

    pub(super) fn within(&self, limits: CephFsJournalReplayLimits) -> bool {
        self.source_spans <= limits.max_source_spans
            && self.provenance_entries <= limits.max_provenance_entries
            && self.retained_bytes <= limits.max_retained_bytes
    }

    pub(super) fn admit(
        &mut self,
        frame: &ceph_wire::CephFsJournalFrame,
        spans: &[CephFsJournalSourceSpan],
        limits: CephFsJournalReplayLimits,
    ) -> Result<(), CephFsJournalStopReason> {
        let source_spans = self
            .source_spans
            .checked_add(spans.len())
            .ok_or(CephFsJournalStopReason::SourceSpanBudget)?;
        if source_spans > limits.max_source_spans {
            return Err(CephFsJournalStopReason::SourceSpanBudget);
        }
        let added_provenance = spans.iter().try_fold(0usize, |total, span| {
            total.checked_add(span.provenance.len())
        });
        let provenance_entries = added_provenance
            .and_then(|added| self.provenance_entries.checked_add(added))
            .ok_or(CephFsJournalStopReason::ProvenanceBudget)?;
        if provenance_entries > limits.max_provenance_entries {
            return Err(CephFsJournalStopReason::ProvenanceBudget);
        }
        let added_bytes = retained_event_bytes(frame, spans)
            .ok_or(CephFsJournalStopReason::RetainedMemoryBudget)?;
        let retained_bytes = self
            .retained_bytes
            .checked_add(added_bytes)
            .ok_or(CephFsJournalStopReason::RetainedMemoryBudget)?;
        if retained_bytes > limits.max_retained_bytes {
            return Err(CephFsJournalStopReason::RetainedMemoryBudget);
        }
        self.source_spans = source_spans;
        self.provenance_entries = provenance_entries;
        self.retained_bytes = retained_bytes;
        Ok(())
    }
}

fn retained_event_bytes(
    frame: &ceph_wire::CephFsJournalFrame,
    spans: &[CephFsJournalSourceSpan],
) -> Option<u64> {
    let fixed =
        std::mem::size_of::<CephFsJournalReplayEvent>().checked_add(frame.payload_sha256.len())?;
    let total = spans.iter().try_fold(fixed, |total, span| {
        usize::try_from(retained_span_bytes(span))
            .ok()
            .and_then(|span_bytes| total.checked_add(span_bytes))
    })?;
    u64::try_from(total).ok()?.checked_mul(2)
}

fn retained_span_bytes(span: &CephFsJournalSourceSpan) -> u64 {
    let fixed = std::mem::size_of::<CephFsJournalSourceSpan>();
    let strings = span.locator.len().saturating_add(span.range_sha256.len());
    let provenance = span.provenance.iter().fold(0usize, |total, source| {
        total
            .saturating_add(std::mem::size_of::<CephFsObjectReadProvenance>())
            .saturating_add(source.data_source_id.len())
            .saturating_add(source.inventory_id.len())
            .saturating_add(source.object_identity_sha256.len())
    });
    u64::try_from(fixed.saturating_add(strings).saturating_add(provenance)).unwrap_or(u64::MAX)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CephFsJournalReplayError {
    #[error("invalid CephFS journal replay limits")]
    InvalidLimits,
    #[error("CephFS journal rank binding is invalid for rank {rank}")]
    InvalidRankBinding { rank: u32 },
    #[error("CephFS journal control object exceeds the {MAX_CONTROL_OBJECT_BYTES} byte limit")]
    ControlObjectTooLarge,
    #[error("CephFS journal control provenance exceeds the retained-state budget")]
    RetainedStateBudget,
    #[error("CephFS journal header pool is not bound to the filesystem descriptor")]
    HeaderPoolMismatch,
    #[error("CephFS journal pointer references an inode outside the bound rank")]
    PointerInodeMismatch,
    #[error(transparent)]
    Inventory(#[from] CephFsInventoryError),
    #[error(transparent)]
    Object(#[from] CephFsObjectReadError),
    #[error(transparent)]
    Wire(#[from] ceph_wire::CephWireError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalPersistenceOutcome {
    Replaced,
    Unchanged,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CephFsJournalPersistenceError {
    #[error("invalid CephFS journal source binding")]
    InvalidSourceBinding,
    #[error("CephFS journal replay digest does not match the in-memory replay")]
    ReplayDigestMismatch,
    #[error("CephFS metadata inventory is unavailable or incomplete")]
    MetadataInventoryUnavailable,
    #[error("CephFS journal {kind} provenance must contain exactly one source span")]
    InvalidControlProvenance { kind: &'static str },
    #[error("CephFS journal span has no provenance for the target source: {locator}")]
    MissingLocalProvenance { locator: String },
    #[error("CephFS journal span has duplicate provenance for the target source: {locator}")]
    DuplicateLocalProvenance { locator: String },
    #[error("CephFS journal projection is not bound to the source metadata inventory")]
    SourceBindingMismatch,
    #[error("CephFS journal projection references an unknown source-local object")]
    ObjectBindingMismatch,
    #[error("CephFS journal projection is non-deterministic for the same input")]
    DeterminismConflict,
    #[error("CephFS journal projection is invalid")]
    InvalidProjection,
    #[error("CephFS journal persistence failed")]
    Database,
}

impl transport::ServiceErrorCategory for CephFsJournalReplayError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::InvalidLimits | Self::InvalidRankBinding { .. } => {
                transport::ErrorCategory::Validation
            }
            Self::Object(error) => transport::ServiceErrorCategory::category(error),
            Self::Inventory(_)
            | Self::Wire(_)
            | Self::HeaderPoolMismatch
            | Self::PointerInodeMismatch => transport::ErrorCategory::Parser,
            Self::ControlObjectTooLarge | Self::RetainedStateBudget => {
                transport::ErrorCategory::Parser
            }
        }
    }
}

impl transport::ServiceErrorCategory for CephFsJournalPersistenceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::InvalidSourceBinding
            | Self::InvalidControlProvenance { .. }
            | Self::MissingLocalProvenance { .. }
            | Self::DuplicateLocalProvenance { .. }
            | Self::ReplayDigestMismatch => transport::ErrorCategory::Validation,
            Self::Database => transport::ErrorCategory::Io,
            Self::MetadataInventoryUnavailable
            | Self::SourceBindingMismatch
            | Self::ObjectBindingMismatch
            | Self::DeterminismConflict
            | Self::InvalidProjection => transport::ErrorCategory::Parser,
        }
    }
}

pub(super) fn control_object_limit() -> u64 {
    MAX_CONTROL_OBJECT_BYTES
}
