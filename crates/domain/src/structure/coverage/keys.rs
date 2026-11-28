use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use super::{require_digest, require_text, validate_partition, CoverageContext, CoverageError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyOrdering {
    pub comparator_id: String,
    pub sort_key_encoding_id: String,
    pub witness_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum KeyBoundary {
    UnboundedStart,
    Before {
        native_key: Vec<u8>,
        sort_key: Vec<u8>,
    },
    After {
        native_key: Vec<u8>,
        sort_key: Vec<u8>,
    },
    UnboundedEnd,
}

impl<'de> Deserialize<'de> for KeyBoundary {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "kind",
            rename_all = "snake_case",
            rename_all_fields = "camelCase",
            deny_unknown_fields
        )]
        enum Record {
            UnboundedStart {},
            Before {
                native_key: Vec<u8>,
                sort_key: Vec<u8>,
            },
            After {
                native_key: Vec<u8>,
                sort_key: Vec<u8>,
            },
            UnboundedEnd {},
        }
        Ok(match Record::deserialize(deserializer)? {
            Record::UnboundedStart {} => Self::UnboundedStart,
            Record::Before {
                native_key,
                sort_key,
            } => Self::Before {
                native_key,
                sort_key,
            },
            Record::After {
                native_key,
                sort_key,
            } => Self::After {
                native_key,
                sort_key,
            },
            Record::UnboundedEnd {} => Self::UnboundedEnd,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeKeyRange {
    pub lower: KeyBoundary,
    pub upper: KeyBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyCoverage {
    pub context: CoverageContext,
    pub ordering: KeyOrdering,
    pub requested: Vec<NativeKeyRange>,
    pub verified: Vec<NativeKeyRange>,
    pub tombstones: Vec<NativeKeyRange>,
    pub unknown: Vec<NativeKeyRange>,
    pub unreadable: Vec<NativeKeyRange>,
}

impl KeyCoverage {
    pub fn validate(&self) -> Result<(), CoverageError> {
        self.context.validate()?;
        require_text(&self.ordering.comparator_id, "comparator_id")?;
        require_text(&self.ordering.sort_key_encoding_id, "sort_key_encoding_id")?;
        require_digest(&self.ordering.witness_sha256, "ordering_witness")?;
        if self.requested.is_empty() {
            return Err(CoverageError::EmptyRequest);
        }
        self.validate_key_bindings()?;
        validate_partition(
            &intervals(&self.requested),
            &[
                intervals(&self.verified),
                intervals(&self.tombstones),
                intervals(&self.unknown),
                intervals(&self.unreadable),
            ],
        )
    }

    fn validate_key_bindings(&self) -> Result<(), CoverageError> {
        let mut native_keys = BTreeMap::<&[u8], &[u8]>::new();
        let mut sort_keys = BTreeMap::<&[u8], &[u8]>::new();
        for range in self
            .requested
            .iter()
            .chain(&self.verified)
            .chain(&self.tombstones)
            .chain(&self.unknown)
            .chain(&self.unreadable)
        {
            for boundary in [&range.lower, &range.upper] {
                let (KeyBoundary::Before {
                    native_key,
                    sort_key,
                }
                | KeyBoundary::After {
                    native_key,
                    sort_key,
                }) = boundary
                else {
                    continue;
                };
                if native_keys
                    .insert(native_key, sort_key)
                    .is_some_and(|previous| previous != sort_key.as_slice())
                    || sort_keys
                        .insert(sort_key, native_key)
                        .is_some_and(|previous| previous != native_key.as_slice())
                {
                    return Err(CoverageError::InconsistentKeyOrder);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum OrderedBoundary {
    Start,
    Finite { sort_key: Vec<u8>, after: bool },
    End,
}

fn ordered(boundary: &KeyBoundary) -> OrderedBoundary {
    match boundary {
        KeyBoundary::UnboundedStart => OrderedBoundary::Start,
        KeyBoundary::Before { sort_key, .. } => OrderedBoundary::Finite {
            sort_key: sort_key.clone(),
            after: false,
        },
        KeyBoundary::After { sort_key, .. } => OrderedBoundary::Finite {
            sort_key: sort_key.clone(),
            after: true,
        },
        KeyBoundary::UnboundedEnd => OrderedBoundary::End,
    }
}

fn intervals(ranges: &[NativeKeyRange]) -> Vec<(OrderedBoundary, OrderedBoundary)> {
    ranges
        .iter()
        .map(|range| (ordered(&range.lower), ordered(&range.upper)))
        .collect()
}

impl<'de> Deserialize<'de> for KeyCoverage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            context: CoverageContext,
            ordering: KeyOrdering,
            requested: Vec<NativeKeyRange>,
            verified: Vec<NativeKeyRange>,
            tombstones: Vec<NativeKeyRange>,
            unknown: Vec<NativeKeyRange>,
            unreadable: Vec<NativeKeyRange>,
        }
        let record = Record::deserialize(deserializer)?;
        let value = Self {
            context: record.context,
            ordering: record.ordering,
            requested: record.requested,
            verified: record.verified,
            tombstones: record.tombstones,
            unknown: record.unknown,
            unreadable: record.unreadable,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
