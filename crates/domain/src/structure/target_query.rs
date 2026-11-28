use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::{
    target::{require_target_identity, validate_target_range},
    ByteRange, TargetSelectorError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetKeyBound {
    pub key: Vec<u8>,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetKeyOrdering {
    Bytewise,
    Native {
        #[serde(rename = "comparatorId")]
        comparator_id: String,
    },
}

impl<'de> Deserialize<'de> for TargetKeyOrdering {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Record {
            Bytewise {},
            Native {
                #[serde(rename = "comparatorId")]
                comparator_id: String,
            },
        }
        match Record::deserialize(deserializer)? {
            Record::Bytewise {} => Ok(Self::Bytewise),
            Record::Native { comparator_id } => {
                require_target_identity(&comparator_id, "comparator_id")
                    .map_err(serde::de::Error::custom)?;
                Ok(Self::Native { comparator_id })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetKeyRange {
    All,
    Exact {
        key: Vec<u8>,
    },
    Interval {
        lower: Option<TargetKeyBound>,
        upper: Option<TargetKeyBound>,
        ordering: TargetKeyOrdering,
    },
}

impl TargetKeyRange {
    pub fn validate(&self) -> Result<(), TargetSelectorError> {
        let Self::Interval {
            lower,
            upper,
            ordering,
        } = self
        else {
            return Ok(());
        };
        if lower.is_none() && upper.is_none() {
            return Err(TargetSelectorError::UnboundedKeyInterval);
        }
        match ordering {
            TargetKeyOrdering::Native { comparator_id } => {
                require_target_identity(comparator_id, "comparator_id")?;
            }
            TargetKeyOrdering::Bytewise => {
                if let (Some(lower), Some(upper)) = (lower, upper) {
                    if lower.key > upper.key
                        || lower.key == upper.key && !(lower.inclusive && upper.inclusive)
                    {
                        return Err(TargetSelectorError::InvalidKeyInterval);
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for TargetKeyRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Record {
            All {},
            Exact {
                key: Vec<u8>,
            },
            Interval {
                lower: Option<TargetKeyBound>,
                upper: Option<TargetKeyBound>,
                ordering: TargetKeyOrdering,
            },
        }
        let range = match Record::deserialize(deserializer)? {
            Record::All {} => Self::All,
            Record::Exact { key } => Self::Exact { key },
            Record::Interval {
                lower,
                upper,
                ordering,
            } => Self::Interval {
                lower,
                upper,
                ordering,
            },
        };
        range.validate().map_err(serde::de::Error::custom)?;
        Ok(range)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NamespaceFilter {
    NamePrefix { value: String },
    PathPrefix { value: String },
    EntryKind { native_kind: String },
    NativePredicate { predicate_id: String, value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NamespaceOrdering {
    Native {
        #[serde(rename = "orderingId")]
        ordering_id: String,
    },
    NameAscending,
    NameDescending,
}

impl<'de> Deserialize<'de> for NamespaceOrdering {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Record {
            Native {
                #[serde(rename = "orderingId")]
                ordering_id: String,
            },
            NameAscending {},
            NameDescending {},
        }
        match Record::deserialize(deserializer)? {
            Record::Native { ordering_id } => {
                require_target_identity(&ordering_id, "ordering_id")
                    .map_err(serde::de::Error::custom)?;
                Ok(Self::Native { ordering_id })
            }
            Record::NameAscending {} => Ok(Self::NameAscending),
            Record::NameDescending {} => Ok(Self::NameDescending),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamespaceQuery {
    pub filters: BTreeSet<NamespaceFilter>,
    pub fields: BTreeSet<String>,
    pub ordering: NamespaceOrdering,
    pub page_size: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl NamespaceQuery {
    pub fn validate(&self) -> Result<(), TargetSelectorError> {
        if self.page_size == 0 {
            return Err(TargetSelectorError::EmptyPage);
        }
        if self.fields.is_empty() {
            return Err(TargetSelectorError::EmptyProjection);
        }
        for field in &self.fields {
            require_target_identity(field, "projection_field")?;
        }
        if let Some(cursor) = &self.cursor {
            require_target_identity(cursor, "cursor")?;
        }
        if let NamespaceOrdering::Native { ordering_id } = &self.ordering {
            require_target_identity(ordering_id, "ordering_id")?;
        }
        for filter in &self.filters {
            match filter {
                NamespaceFilter::NamePrefix { value } | NamespaceFilter::PathPrefix { value } => {
                    if value.is_empty() {
                        return Err(TargetSelectorError::EmptyField { field: "prefix" });
                    }
                }
                NamespaceFilter::EntryKind { native_kind } => {
                    require_target_identity(native_kind, "native_kind")?;
                }
                NamespaceFilter::NativePredicate { predicate_id, .. } => {
                    require_target_identity(predicate_id, "predicate_id")?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetSequenceRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_inclusive: Option<u64>,
}

impl TargetSequenceRange {
    pub fn validate(&self) -> Result<(), TargetSelectorError> {
        if let (Some(first), Some(last)) = (self.first, self.last_inclusive) {
            if first > last {
                return Err(TargetSelectorError::InvalidSequenceInterval);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetEventClock {
    Event,
    Observation,
    Reception,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetEventTimeRange {
    pub clock: TargetEventClock,
    pub start_unix_ms: i64,
    pub end_unix_ms: i64,
}

impl TargetEventTimeRange {
    pub fn validate(&self) -> Result<(), TargetSelectorError> {
        if self.start_unix_ms >= self.end_unix_ms {
            return Err(TargetSelectorError::InvalidTimeInterval);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetEventOrdering {
    NativeSequence,
    EventTime,
    ObservationTime,
    ReceptionTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeScope {
    Snapshot,
    Process {
        process_id: u64,
    },
    Region {
        #[serde(skip_serializing_if = "Option::is_none")]
        process_id: Option<u64>,
        region_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<ByteRange>,
    },
}

impl RuntimeScope {
    pub fn validate(&self) -> Result<(), TargetSelectorError> {
        if let Self::Region {
            region_id, range, ..
        } = self
        {
            require_target_identity(region_id, "region_id")?;
            if let Some(range) = range {
                validate_target_range(*range)?;
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for RuntimeScope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "kind",
            rename_all = "snake_case",
            rename_all_fields = "camelCase",
            deny_unknown_fields
        )]
        enum Record {
            Snapshot {},
            Process {
                process_id: u64,
            },
            Region {
                process_id: Option<u64>,
                region_id: String,
                range: Option<ByteRange>,
            },
        }
        let scope = match Record::deserialize(deserializer)? {
            Record::Snapshot {} => Self::Snapshot,
            Record::Process { process_id } => Self::Process { process_id },
            Record::Region {
                process_id,
                region_id,
                range,
            } => Self::Region {
                process_id,
                region_id,
                range,
            },
        };
        scope.validate().map_err(serde::de::Error::custom)?;
        Ok(scope)
    }
}
