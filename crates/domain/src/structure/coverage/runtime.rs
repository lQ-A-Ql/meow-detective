use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::{require_text, CoverageContext, CoverageError};
use crate::structure::ByteRange;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeComponent {
    Process {
        process_id: String,
    },
    MemoryRegion {
        process_id: String,
        range: ByteRange,
    },
    NativeState {
        native_kind: String,
        native_id: String,
    },
}

impl RuntimeComponent {
    pub fn validate(&self) -> Result<(), CoverageError> {
        match self {
            Self::Process { process_id } => require_text(process_id, "process_id"),
            Self::MemoryRegion { process_id, range } => {
                require_text(process_id, "process_id")?;
                if range.is_empty() || range.start().checked_add(range.length()).is_none() {
                    return Err(CoverageError::InvalidRuntimeComponent);
                }
                Ok(())
            }
            Self::NativeState {
                native_kind,
                native_id,
            } => {
                require_text(native_kind, "runtime_kind")?;
                require_text(native_id, "runtime_id")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCoverage {
    pub context: CoverageContext,
    pub snapshot_id: String,
    pub requested: BTreeSet<RuntimeComponent>,
    pub captured: BTreeSet<RuntimeComponent>,
    pub not_captured: BTreeSet<RuntimeComponent>,
    pub unavailable: BTreeSet<RuntimeComponent>,
    pub unknown: BTreeSet<RuntimeComponent>,
}

impl RuntimeCoverage {
    pub fn validate(&self) -> Result<(), CoverageError> {
        self.context.validate()?;
        require_text(&self.snapshot_id, "snapshot_id")?;
        if self.requested.is_empty() {
            return Err(CoverageError::EmptyRequest);
        }
        let mut previous_region: Option<(&str, u64)> = None;
        for component in &self.requested {
            component.validate()?;
            if let RuntimeComponent::MemoryRegion { process_id, range } = component {
                if previous_region
                    .is_some_and(|(previous, end)| previous == process_id && end > range.start())
                {
                    return Err(CoverageError::NoncanonicalRanges);
                }
                previous_region = Some((process_id, range.start() + range.length()));
            }
        }
        let mut classified = BTreeSet::new();
        for class in [
            &self.captured,
            &self.not_captured,
            &self.unavailable,
            &self.unknown,
        ] {
            for component in class {
                component.validate()?;
                if !classified.insert(component) {
                    return Err(CoverageError::ContradictoryClassification);
                }
            }
        }
        if classified != self.requested.iter().collect() {
            return Err(CoverageError::IncompletePartition);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for RuntimeCoverage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            context: CoverageContext,
            snapshot_id: String,
            requested: BTreeSet<RuntimeComponent>,
            captured: BTreeSet<RuntimeComponent>,
            not_captured: BTreeSet<RuntimeComponent>,
            unavailable: BTreeSet<RuntimeComponent>,
            unknown: BTreeSet<RuntimeComponent>,
        }
        let record = Record::deserialize(deserializer)?;
        let value = Self {
            context: record.context,
            snapshot_id: record.snapshot_id,
            requested: record.requested,
            captured: record.captured,
            not_captured: record.not_captured,
            unavailable: record.unavailable,
            unknown: record.unknown,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
