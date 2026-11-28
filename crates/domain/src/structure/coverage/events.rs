use serde::{Deserialize, Deserializer, Serialize};

use super::{require_text, validate_partition, CoverageContext, CoverageError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSequenceRange {
    pub first: u64,
    pub last_inclusive: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOrdering {
    NativeSequence,
    EventTime,
    ObservationTime,
    ReceiptTime,
    Unordered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum EventClockBasis {
    Utc,
    FixedOffset { seconds: i32 },
    TimezoneUnknown,
    Monotonic { clock_id: String },
}

impl<'de> Deserialize<'de> for EventClockBasis {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "kind",
            rename_all = "snake_case",
            rename_all_fields = "camelCase",
            deny_unknown_fields
        )]
        enum Record {
            Utc {},
            FixedOffset { seconds: i32 },
            TimezoneUnknown {},
            Monotonic { clock_id: String },
        }
        Ok(match Record::deserialize(deserializer)? {
            Record::Utc {} => Self::Utc,
            Record::FixedOffset { seconds } => Self::FixedOffset { seconds },
            Record::TimezoneUnknown {} => Self::TimezoneUnknown,
            Record::Monotonic { clock_id } => Self::Monotonic { clock_id },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventClock {
    pub basis: EventClockBasis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertainty_nanoseconds: Option<u64>,
}

impl EventClock {
    pub fn validate(&self) -> Result<(), CoverageError> {
        match &self.basis {
            EventClockBasis::FixedOffset { seconds } if seconds.abs_diff(0) >= 86400 => {
                Err(CoverageError::InvalidClock)
            }
            EventClockBasis::Monotonic { clock_id } => require_text(clock_id, "clock_id"),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventCoverage {
    pub context: CoverageContext,
    pub sequence_domain: String,
    pub native_ordering_id: String,
    pub requested: Vec<NativeSequenceRange>,
    pub verified: Vec<NativeSequenceRange>,
    pub native_gaps: Vec<NativeSequenceRange>,
    pub unknown: Vec<NativeSequenceRange>,
    pub unreadable: Vec<NativeSequenceRange>,
    pub ordering: EventOrdering,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_clock: Option<EventClock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_clock: Option<EventClock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_clock: Option<EventClock>,
}

impl EventCoverage {
    pub fn validate(&self) -> Result<(), CoverageError> {
        self.context.validate()?;
        require_text(&self.sequence_domain, "sequence_domain")?;
        require_text(&self.native_ordering_id, "native_ordering_id")?;
        if self.requested.is_empty() {
            return Err(CoverageError::EmptyRequest);
        }
        for clock in [
            &self.event_clock,
            &self.observation_clock,
            &self.receipt_clock,
        ]
        .into_iter()
        .flatten()
        {
            clock.validate()?;
        }
        match self.ordering {
            EventOrdering::EventTime if self.event_clock.is_none() => {
                return Err(CoverageError::MissingClock);
            }
            EventOrdering::ObservationTime if self.observation_clock.is_none() => {
                return Err(CoverageError::MissingClock);
            }
            EventOrdering::ReceiptTime if self.receipt_clock.is_none() => {
                return Err(CoverageError::MissingClock);
            }
            _ => {}
        }
        validate_partition(
            &intervals(&self.requested),
            &[
                intervals(&self.verified),
                intervals(&self.native_gaps),
                intervals(&self.unknown),
                intervals(&self.unreadable),
            ],
        )
    }
}

fn intervals(ranges: &[NativeSequenceRange]) -> Vec<(u128, u128)> {
    ranges
        .iter()
        .map(|range| {
            (
                u128::from(range.first),
                u128::from(range.last_inclusive) + 1,
            )
        })
        .collect()
}

impl<'de> Deserialize<'de> for EventCoverage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            context: CoverageContext,
            sequence_domain: String,
            native_ordering_id: String,
            requested: Vec<NativeSequenceRange>,
            verified: Vec<NativeSequenceRange>,
            native_gaps: Vec<NativeSequenceRange>,
            unknown: Vec<NativeSequenceRange>,
            unreadable: Vec<NativeSequenceRange>,
            ordering: EventOrdering,
            event_clock: Option<EventClock>,
            observation_clock: Option<EventClock>,
            receipt_clock: Option<EventClock>,
        }
        let record = Record::deserialize(deserializer)?;
        let value = Self {
            context: record.context,
            sequence_domain: record.sequence_domain,
            native_ordering_id: record.native_ordering_id,
            requested: record.requested,
            verified: record.verified,
            native_gaps: record.native_gaps,
            unknown: record.unknown,
            unreadable: record.unreadable,
            ordering: record.ordering,
            event_clock: record.event_clock,
            observation_clock: record.observation_clock,
            receipt_clock: record.receipt_clock,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
