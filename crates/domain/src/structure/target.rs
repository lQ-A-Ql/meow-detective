use serde::{Deserialize, Deserializer, Serialize};

use crate::FileEntryId;

use super::{
    canonical_identity, ByteRange, ByteRangeError, CanonicalIdentityError, VersionBoundary,
    VersionSelectionError,
};

pub use super::target_query::{
    NamespaceFilter, NamespaceOrdering, NamespaceQuery, RuntimeScope, TargetEventClock,
    TargetEventOrdering, TargetEventTimeRange, TargetKeyBound, TargetKeyOrdering, TargetKeyRange,
    TargetSequenceRange,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TargetSelector {
    Block {
        view_id: String,
        range: ByteRange,
    },
    File {
        view_id: String,
        namespace_snapshot: String,
        file_entry_id: FileEntryId,
        #[serde(skip_serializing_if = "Option::is_none")]
        stream: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<ByteRange>,
    },
    Object {
        view_id: String,
        native_namespace: String,
        object_id: String,
        object_version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<ByteRange>,
    },
    Key {
        view_id: String,
        native_namespace: String,
        range: TargetKeyRange,
        boundary: VersionBoundary,
    },
    Namespace {
        view_id: String,
        namespace_snapshot: String,
        root_id: String,
        query: NamespaceQuery,
    },
    Event {
        view_id: String,
        sequence_domain: String,
        boundary: VersionBoundary,
        range: TargetSequenceRange,
        #[serde(skip_serializing_if = "Option::is_none")]
        time: Option<TargetEventTimeRange>,
        ordering: TargetEventOrdering,
    },
    Runtime {
        view_id: String,
        snapshot_id: String,
        state_kind: String,
        scope: RuntimeScope,
    },
}

impl TargetSelector {
    pub fn view_id(&self) -> &str {
        match self {
            Self::Block { view_id, .. }
            | Self::File { view_id, .. }
            | Self::Object { view_id, .. }
            | Self::Key { view_id, .. }
            | Self::Namespace { view_id, .. }
            | Self::Event { view_id, .. }
            | Self::Runtime { view_id, .. } => view_id,
        }
    }

    pub fn validate(&self) -> Result<(), TargetSelectorError> {
        require_target_identity(self.view_id(), "view_id")?;
        match self {
            Self::Block { range, .. } => validate_target_range(*range)?,
            Self::File {
                namespace_snapshot,
                file_entry_id,
                stream,
                range,
                ..
            } => {
                require_target_identity(namespace_snapshot, "namespace_snapshot")?;
                require_target_identity(&file_entry_id.0, "file_entry_id")?;
                if stream.as_ref().is_some_and(String::is_empty) {
                    return Err(TargetSelectorError::EmptyField { field: "stream" });
                }
                if let Some(range) = range {
                    validate_target_range(*range)?;
                }
            }
            Self::Object {
                object_id,
                object_version,
                range,
                ..
            } => {
                require_target_identity(object_id, "object_id")?;
                require_target_identity(object_version, "object_version")?;
                if let Some(range) = range {
                    validate_target_range(*range)?;
                }
            }
            Self::Key {
                range, boundary, ..
            } => {
                boundary.validate()?;
                range.validate()?;
            }
            Self::Namespace {
                namespace_snapshot,
                root_id,
                query,
                ..
            } => {
                require_target_identity(namespace_snapshot, "namespace_snapshot")?;
                require_target_identity(root_id, "root_id")?;
                query.validate()?;
            }
            Self::Event {
                sequence_domain,
                boundary,
                range,
                time,
                ..
            } => {
                require_target_identity(sequence_domain, "sequence_domain")?;
                boundary.validate()?;
                range.validate()?;
                if let Some(time) = time {
                    time.validate()?;
                }
            }
            Self::Runtime {
                snapshot_id,
                state_kind,
                scope,
                ..
            } => {
                require_target_identity(snapshot_id, "snapshot_id")?;
                require_target_identity(state_kind, "state_kind")?;
                scope.validate()?;
            }
        }
        Ok(())
    }

    pub fn identity(&self) -> Result<String, TargetSelectorError> {
        self.validate()?;
        Ok(canonical_identity("target-selector-v1", self)?)
    }
}

impl<'de> Deserialize<'de> for TargetSelector {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let selector = TargetSelectorRecord::deserialize(deserializer)?;
        selector.validate().map_err(serde::de::Error::custom)?;
        Ok(selector)
    }
}

#[derive(Deserialize)]
#[serde(
    remote = "TargetSelector",
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum TargetSelectorRecord {
    Block {
        view_id: String,
        range: ByteRange,
    },
    File {
        view_id: String,
        namespace_snapshot: String,
        file_entry_id: FileEntryId,
        stream: Option<String>,
        range: Option<ByteRange>,
    },
    Object {
        view_id: String,
        native_namespace: String,
        object_id: String,
        object_version: String,
        range: Option<ByteRange>,
    },
    Key {
        view_id: String,
        native_namespace: String,
        range: TargetKeyRange,
        boundary: VersionBoundary,
    },
    Namespace {
        view_id: String,
        namespace_snapshot: String,
        root_id: String,
        query: NamespaceQuery,
    },
    Event {
        view_id: String,
        sequence_domain: String,
        boundary: VersionBoundary,
        range: TargetSequenceRange,
        time: Option<TargetEventTimeRange>,
        ordering: TargetEventOrdering,
    },
    Runtime {
        view_id: String,
        snapshot_id: String,
        state_kind: String,
        scope: RuntimeScope,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum TargetSelectorError {
    #[error("target selector field '{field}' is empty")]
    EmptyField { field: &'static str },
    #[error("namespace page size must be nonzero")]
    EmptyPage,
    #[error("namespace query must explicitly select its output fields")]
    EmptyProjection,
    #[error("native key interval is empty or reversed")]
    InvalidKeyInterval,
    #[error("an unbounded key range must use the all variant")]
    UnboundedKeyInterval,
    #[error("native sequence interval is reversed")]
    InvalidSequenceInterval,
    #[error("event time interval is empty or reversed")]
    InvalidTimeInterval,
    #[error(transparent)]
    Range(#[from] ByteRangeError),
    #[error(transparent)]
    Version(#[from] VersionSelectionError),
    #[error(transparent)]
    Identity(#[from] CanonicalIdentityError),
}

pub(super) fn require_target_identity(
    value: &str,
    field: &'static str,
) -> Result<(), TargetSelectorError> {
    if value.trim().is_empty() {
        return Err(TargetSelectorError::EmptyField { field });
    }
    Ok(())
}

pub(super) fn validate_target_range(range: ByteRange) -> Result<(), TargetSelectorError> {
    ByteRange::new(range.start(), range.length())?;
    Ok(())
}
