use crate::error::{CephWireError, Result};

pub const CEPHFS_JOURNAL_MAGIC: &str = "ceph fs volume v011";
pub const CEPHFS_JOURNAL_MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalStreamFormat {
    Legacy,
    Resilient,
}

impl CephFsJournalStreamFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Resilient => "resilient",
        }
    }
}

impl TryFrom<u8> for CephFsJournalStreamFormat {
    type Error = CephWireError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Legacy),
            1 => Ok(Self::Resilient),
            value => Err(CephWireError::UnsupportedCephFsJournalStreamFormat { value }),
        }
    }
}

impl From<CephFsJournalStreamFormat> for u8 {
    fn from(value: CephFsJournalStreamFormat) -> Self {
        match value {
            CephFsJournalStreamFormat::Legacy => 0,
            CephFsJournalStreamFormat::Resilient => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephFsJournalPointer {
    pub front: u64,
    pub back: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalHeader {
    pub magic: String,
    pub trimmed_pos: u64,
    pub expire_pos: u64,
    pub unused_pos: u64,
    pub write_pos: u64,
    pub layout: CephFsJournalLayout,
    pub stream_format: CephFsJournalStreamFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephFsJournalLayout {
    pub stripe_unit: u32,
    pub stripe_count: u32,
    pub object_size: u32,
    pub pool_id: i64,
}

impl CephFsJournalLayout {
    pub fn period(self) -> Result<u64> {
        u64::from(self.stripe_count)
            .checked_mul(u64::from(self.object_size))
            .ok_or(CephWireError::CephFsJournalRangeOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephFsJournalObjectExtent {
    pub logical_offset: u64,
    pub object_index: u32,
    pub object_offset: u64,
    pub length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephFsJournalFramePrefix {
    pub logical_offset: u64,
    pub prefix_length: usize,
    pub payload_length: usize,
    pub trailer_length: usize,
    pub total_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalFrame {
    pub logical_offset: u64,
    pub logical_end: u64,
    pub payload_length: u32,
    pub payload_sha256: String,
    pub event: CephFsJournalEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalEvent {
    pub encoding: CephFsJournalEventEncoding,
    pub kind: CephFsJournalEventKind,
    pub event_type: u32,
    pub semantic_state: CephFsJournalEventSemanticState,
    pub boundary_sequence: CephFsJournalBoundarySequence,
    pub segment_sequence: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalEventSemanticState {
    Supported,
    UnknownEventType,
    UnsupportedEnvelope {
        structure: &'static str,
        encoded_version: u8,
        compat_version: u8,
    },
    MalformedEnvelope {
        structure: &'static str,
    },
}

impl CephFsJournalEventSemanticState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::UnknownEventType => "unknown_event_type",
            Self::UnsupportedEnvelope { .. } => "unsupported_envelope",
            Self::MalformedEnvelope { .. } => "malformed_envelope",
        }
    }

    pub fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }

    pub fn is_unknown(self) -> bool {
        matches!(self, Self::UnknownEventType)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalBoundarySequence {
    NotBoundary,
    Encoded(u64),
    LogicalOffset,
    Unavailable,
}

impl CephFsJournalBoundarySequence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotBoundary => "not_boundary",
            Self::Encoded(_) => "encoded",
            Self::LogicalOffset => "logical_offset",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn resolve(self, logical_offset: u64) -> Option<u64> {
        match self {
            Self::Encoded(sequence) if sequence != 0 => Some(sequence),
            Self::Encoded(_) | Self::LogicalOffset => Some(logical_offset),
            Self::NotBoundary | Self::Unavailable => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalEventEncoding {
    Legacy,
    Versioned { version: u8, compat_version: u8 },
}

impl CephFsJournalEventEncoding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Versioned { .. } => "versioned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalEventKind {
    SubtreeMap,
    Export,
    ImportStart,
    ImportFinish,
    Fragment,
    ResetJournal,
    Session,
    SessionsOld,
    Sessions,
    Update,
    PeerUpdate,
    Open,
    Committed,
    Purged,
    TableClient,
    TableServer,
    SubtreeMapTest,
    Noop,
    Segment,
    Lid,
    Unknown,
}

impl CephFsJournalEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SubtreeMap => "subtree_map",
            Self::Export => "export",
            Self::ImportStart => "import_start",
            Self::ImportFinish => "import_finish",
            Self::Fragment => "fragment",
            Self::ResetJournal => "reset_journal",
            Self::Session => "session",
            Self::SessionsOld => "sessions_old",
            Self::Sessions => "sessions",
            Self::Update => "update",
            Self::PeerUpdate => "peer_update",
            Self::Open => "open",
            Self::Committed => "committed",
            Self::Purged => "purged",
            Self::TableClient => "table_client",
            Self::TableServer => "table_server",
            Self::SubtreeMapTest => "subtree_map_test",
            Self::Noop => "noop",
            Self::Segment => "segment",
            Self::Lid => "lid",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn from_type(event_type: u32) -> Self {
        match event_type {
            2 => Self::SubtreeMap,
            3 => Self::Export,
            4 => Self::ImportStart,
            5 => Self::ImportFinish,
            6 => Self::Fragment,
            9 => Self::ResetJournal,
            10 => Self::Session,
            11 => Self::SessionsOld,
            12 => Self::Sessions,
            20 => Self::Update,
            21 => Self::PeerUpdate,
            22 => Self::Open,
            23 => Self::Committed,
            24 => Self::Purged,
            42 => Self::TableClient,
            43 => Self::TableServer,
            50 => Self::SubtreeMapTest,
            51 => Self::Noop,
            100 => Self::Segment,
            101 => Self::Lid,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn is_major_boundary(self) -> bool {
        matches!(self, Self::SubtreeMap | Self::ResetJournal | Self::Lid)
    }

    pub fn is_boundary(self) -> bool {
        self.is_major_boundary() || matches!(self, Self::Segment)
    }
}
