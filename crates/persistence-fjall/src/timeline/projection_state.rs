//! Timeline projection state markers stored in the source Fjall Meta keyspace.
//!
//! Family: `timeline_projection_state`; published timelines carry an explicit
//! `complete`, `building`, or `deferred` marker.

use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    errors::Result,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub const TIMELINE_PROJECTION_STATE_FAMILY: &str = "timeline_projection_state";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimelineProjectionState {
    Complete,
    Building,
    Deferred,
}

impl TimelineProjectionState {
    pub fn record_kind(self) -> &'static str {
        match self {
            Self::Complete => "timeline_projection_complete",
            Self::Building => "timeline_projection_building",
            Self::Deferred => "timeline_projection_deferred",
        }
    }

    pub fn from_record_kind(kind: &str) -> Option<Self> {
        match kind {
            "timeline_projection_complete" => Some(Self::Complete),
            "timeline_projection_building" => Some(Self::Building),
            "timeline_projection_deferred" => Some(Self::Deferred),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Building => "building",
            Self::Deferred => "deferred",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineProjectionStatePayload {
    pub status: TimelineProjectionState,
    pub identity: String,
    pub inserted_events: u64,
    pub required_bytes: u64,
    pub available_bytes: u64,
    pub reason: Option<String>,
    pub updated_at: String,
}

impl TimelineProjectionStatePayload {
    pub fn new(
        status: TimelineProjectionState,
        identity: impl Into<String>,
        inserted_events: u64,
        required_bytes: u64,
        available_bytes: u64,
        reason: Option<String>,
    ) -> Self {
        Self {
            status,
            identity: identity.into(),
            inserted_events,
            required_bytes,
            available_bytes,
            reason,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentLimits {
    pub max_bytes: u64,
    pub max_records: usize,
}

impl SegmentLimits {
    pub const fn default_limits() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_records: 100_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentedReplaceOutcome {
    pub inserted_records: u64,
    pub deleted_records: u64,
    pub segments: u64,
    pub max_segment_bytes: u64,
    pub native_writes: crate::NativeWriteReport,
}

pub fn timeline_projection_state_key(source_id: &str) -> Result<Vec<u8>> {
    encode_key(
        TIMELINE_PROJECTION_STATE_FAMILY,
        &[KeyPart::Text(source_id.to_owned())],
    )
}

pub fn timeline_projection_marker_envelope(
    source_id: &str,
    payload: &TimelineProjectionStatePayload,
) -> Result<(Vec<u8>, VersionedEnvelope)> {
    let key = timeline_projection_state_key(source_id)?;
    let envelope = VersionedEnvelope::from_serializable(
        payload.status.record_kind(),
        1,
        format!("timeline-projection:{source_id}"),
        source_id,
        1,
        payload,
    )?;
    Ok((key, envelope))
}

pub fn decode_timeline_projection_state(
    envelope: &VersionedEnvelope,
) -> Result<Option<TimelineProjectionStatePayload>> {
    let Some(status) = TimelineProjectionState::from_record_kind(&envelope.record_kind) else {
        return Ok(None);
    };
    let mut payload: TimelineProjectionStatePayload = envelope.decode_payload()?;
    payload.status = status;
    Ok(Some(payload))
}
