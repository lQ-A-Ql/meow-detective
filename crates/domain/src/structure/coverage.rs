mod bytes;
mod events;
mod keys;
mod namespace;
mod runtime;

use serde::{Deserialize, Deserializer, Serialize};

use super::VersionBoundary;

pub use bytes::ByteCoverage;
pub use events::{EventClock, EventClockBasis, EventCoverage, EventOrdering, NativeSequenceRange};
pub use keys::{KeyBoundary, KeyCoverage, KeyOrdering, NativeKeyRange};
pub use namespace::{NamespaceCoverage, NamespacePage, NamespacePageEnd};
pub use runtime::{RuntimeComponent, RuntimeCoverage};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "space", content = "coverage", rename_all = "snake_case")]
pub enum TargetCoverage {
    Bytes(ByteCoverage),
    Namespace(NamespaceCoverage),
    Keys(KeyCoverage),
    Events(EventCoverage),
    Runtime(RuntimeCoverage),
}

impl TargetCoverage {
    pub fn validate(&self) -> Result<(), CoverageError> {
        match self {
            Self::Bytes(value) => value.validate(),
            Self::Namespace(value) => value.validate(),
            Self::Keys(value) => value.validate(),
            Self::Events(value) => value.validate(),
            Self::Runtime(value) => value.validate(),
        }
    }

    pub fn is_complete(&self) -> Result<bool, CoverageError> {
        self.validate()?;
        Ok(match self {
            Self::Bytes(value) => value.unknown.is_empty() && value.unreadable.is_empty(),
            Self::Namespace(value) => value.has_verified_eof(),
            Self::Keys(value) => value.unknown.is_empty() && value.unreadable.is_empty(),
            Self::Events(value) => value.unknown.is_empty() && value.unreadable.is_empty(),
            Self::Runtime(value) => {
                value.not_captured.is_empty()
                    && value.unavailable.is_empty()
                    && value.unknown.is_empty()
            }
        })
    }

    pub fn has_proven_portion(&self) -> Result<bool, CoverageError> {
        self.validate()?;
        Ok(match self {
            Self::Bytes(value) => !value.verified.is_empty() || !value.native_holes.is_empty(),
            Self::Namespace(value) => value.pages.iter().any(|page| page.verified),
            Self::Keys(value) => !value.verified.is_empty() || !value.tombstones.is_empty(),
            Self::Events(value) => !value.verified.is_empty() || !value.native_gaps.is_empty(),
            Self::Runtime(value) => !value.captured.is_empty(),
        })
    }

    pub fn view_id(&self) -> Option<&str> {
        match self {
            Self::Bytes(_) => None,
            Self::Namespace(value) => Some(&value.context.view_id),
            Self::Keys(value) => Some(&value.context.view_id),
            Self::Events(value) => Some(&value.context.view_id),
            Self::Runtime(value) => Some(&value.context.view_id),
        }
    }
}

impl<'de> Deserialize<'de> for TargetCoverage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "space",
            content = "coverage",
            rename_all = "snake_case",
            deny_unknown_fields
        )]
        enum Record {
            Bytes(ByteCoverage),
            Namespace(NamespaceCoverage),
            Keys(KeyCoverage),
            Events(EventCoverage),
            Runtime(RuntimeCoverage),
        }
        Ok(match Record::deserialize(deserializer)? {
            Record::Bytes(value) => Self::Bytes(value),
            Record::Namespace(value) => Self::Namespace(value),
            Record::Keys(value) => Self::Keys(value),
            Record::Events(value) => Self::Events(value),
            Record::Runtime(value) => Self::Runtime(value),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverageContext {
    pub view_id: String,
    pub native_namespace: String,
    pub version: VersionBoundary,
}

impl CoverageContext {
    pub fn validate(&self) -> Result<(), CoverageError> {
        require_text(&self.view_id, "view_id")?;
        require_text(&self.native_namespace, "native_namespace")?;
        self.version
            .validate()
            .map_err(|_| CoverageError::InvalidVersionBoundary)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoverageError {
    #[error("coverage field '{field}' must contain a nonempty value")]
    EmptyField { field: &'static str },
    #[error("coverage digest '{field}' must be a lowercase SHA-256 value")]
    InvalidDigest { field: &'static str },
    #[error("coverage native version boundary is invalid")]
    InvalidVersionBoundary,
    #[error("coverage contains an invalid or overflowing interval")]
    InvalidRange,
    #[error("coverage interval exceeds the target")]
    OutOfBounds,
    #[error("coverage requires an explicit request")]
    EmptyRequest,
    #[error("coverage intervals must be ordered and nonoverlapping")]
    NoncanonicalRanges,
    #[error("coverage classifications overlap")]
    ContradictoryClassification,
    #[error("coverage classifications do not partition the request")]
    IncompletePartition,
    #[error("namespace cursor chain is incomplete or repeated")]
    InvalidCursorChain,
    #[error("namespace page ordinals or object counts are invalid")]
    InvalidPageCount,
    #[error("namespace EOF must be verified and occur only on the final page")]
    InvalidEof,
    #[error("namespace continuation follows an unverified page")]
    UnverifiedContinuation,
    #[error("native key identities disagree with their ordering witnesses")]
    InconsistentKeyOrder,
    #[error("event ordering has no corresponding clock metadata")]
    MissingClock,
    #[error("event clock metadata is invalid")]
    InvalidClock,
    #[error("runtime component identity or region is invalid")]
    InvalidRuntimeComponent,
}

pub(super) fn require_text(value: &str, field: &'static str) -> Result<(), CoverageError> {
    if value.trim().is_empty() {
        return Err(CoverageError::EmptyField { field });
    }
    Ok(())
}

pub(super) fn require_digest(value: &str, field: &'static str) -> Result<(), CoverageError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CoverageError::InvalidDigest { field });
    }
    Ok(())
}

fn validate_order<T: Ord>(ranges: &[(T, T)]) -> Result<(), CoverageError> {
    for (index, (start, end)) in ranges.iter().enumerate() {
        if start >= end {
            return Err(CoverageError::InvalidRange);
        }
        if index > 0 && &ranges[index - 1].1 > start {
            return Err(CoverageError::NoncanonicalRanges);
        }
    }
    Ok(())
}

fn validate_partition<T: Ord + Clone>(
    requested: &[(T, T)],
    classes: &[Vec<(T, T)>],
) -> Result<(), CoverageError> {
    validate_order(requested)?;
    let mut requested_union: Vec<(T, T)> = Vec::new();
    for (start, end) in requested {
        if let Some(previous) = requested_union.last_mut() {
            if &previous.1 == start {
                previous.1 = end.clone();
                continue;
            }
        }
        requested_union.push((start.clone(), end.clone()));
    }
    let mut portions = Vec::new();
    for ranges in classes {
        validate_order(ranges)?;
        portions.extend(ranges.iter().cloned());
    }
    portions.sort();
    if portions.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(CoverageError::ContradictoryClassification);
    }
    let mut portions = portions.into_iter().peekable();
    for (start, end) in requested_union {
        let mut cursor = start;
        while cursor < end {
            let Some((portion_start, portion_end)) = portions.next() else {
                return Err(CoverageError::IncompletePartition);
            };
            if portion_start != cursor || portion_end > end {
                return Err(CoverageError::IncompletePartition);
            }
            cursor = portion_end;
        }
    }
    if portions.next().is_some() {
        return Err(CoverageError::IncompletePartition);
    }
    Ok(())
}
