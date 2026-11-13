//! Borrowed lookup index for pending analysis output replacements.
//!
//! Replacement deletes used to test every persisted artifact and timeline
//! record against every pending replacement, which is quadratic in the number
//! of records and replacements. `ReplacementIndex` groups replacements by
//! `source_object_id`, so a record costs one hash lookup plus one producer
//! check per replacement registered for that same source object.

use super::state::{AnalysisOutputProducer, AnalysisOutputReplacement};
use persistence_source::fjall::{ArtifactRecord, TimelineRecord};
use std::collections::{HashMap, HashSet};

const EXTRACTOR_ID_ATTR: &str = "extractorId";

/// Deduplicating collector for pending replacements that preserves first
/// insertion order. `HashSet` membership keeps insertion near-linear where the
/// previous `Vec::contains` scan was quadratic.
#[derive(Debug, Default)]
pub(super) struct ReplacementSet {
    seen: HashSet<AnalysisOutputReplacement>,
    ordered: Vec<AnalysisOutputReplacement>,
}

impl ReplacementSet {
    pub(super) fn insert(&mut self, replacement: AnalysisOutputReplacement) {
        if self.seen.insert(replacement.clone()) {
            self.ordered.push(replacement);
        }
    }

    pub(super) fn into_vec(self) -> Vec<AnalysisOutputReplacement> {
        self.ordered
    }
}

/// Borrowed producer matcher: built-in capabilities replace by extractor-id
/// prefix (`registry.`, `browser.`, ...), plugin outputs carry the bare plugin
/// id and are replaced by exact id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProducerMatch<'a> {
    Prefix(&'a str),
    Exact(&'a str),
}

impl ProducerMatch<'_> {
    pub(super) fn matches(self, producer: &str) -> bool {
        match self {
            Self::Prefix(prefix) => producer.starts_with(prefix),
            Self::Exact(id) => producer == id,
        }
    }

    pub(super) fn matches_opt(self, producer: Option<&str>) -> bool {
        producer.is_some_and(|producer| self.matches(producer))
    }
}

/// `source_object_id`-keyed index over the pending replacements. The keys and
/// producer matchers borrow the replacement slice; no id string is cloned.
pub(super) struct ReplacementIndex<'a> {
    by_source: HashMap<&'a str, Vec<ProducerMatch<'a>>>,
}

impl<'a> ReplacementIndex<'a> {
    pub(super) fn new(replacements: &'a [AnalysisOutputReplacement]) -> Self {
        let mut by_source: HashMap<&'a str, Vec<ProducerMatch<'a>>> = HashMap::new();
        for replacement in replacements {
            let producer = match &replacement.producer {
                AnalysisOutputProducer::Prefix(prefix) => ProducerMatch::Prefix(prefix),
                AnalysisOutputProducer::ExtractorId(id) => ProducerMatch::Exact(id.as_str()),
            };
            by_source
                .entry(replacement.source_object_id.as_str())
                .or_default()
                .push(producer);
        }
        Self { by_source }
    }

    /// Same predicate as the pre-index per-replacement matcher: the record must
    /// belong to `source_id` and its `extractorId` attribute must match one of
    /// the replacement producers registered for its source object.
    pub(super) fn matches_artifact(&self, record: &ArtifactRecord, source_id: &str) -> bool {
        if record.data_source_id != source_id {
            return false;
        }
        let source_object_id = record.source_object_id.as_deref().unwrap_or_default();
        let Some(producers) = self.by_source.get(source_object_id) else {
            return false;
        };
        let producer = record
            .attrs
            .get(EXTRACTOR_ID_ATTR)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        producers.iter().any(|matcher| {
            matcher.matches(producer)
                || (producer.is_empty()
                    && matches!(matcher, ProducerMatch::Prefix(prefix)
                        if legacy_native_family_matches(prefix, &record.artifact_type)))
        })
    }

    /// Same predicate as the pre-index timeline matcher: `parser_id` is
    /// matched by prefix or exact id, and a missing `parser_id` never matches.
    pub(super) fn matches_timeline(&self, record: &TimelineRecord) -> bool {
        let Some(producers) = self.by_source.get(record.source_object_id.as_str()) else {
            return false;
        };
        let parser_id = record.parser_id.as_deref();
        producers
            .iter()
            .any(|matcher| matcher.matches_opt(parser_id))
    }
}

fn legacy_native_family_matches(prefix: &str, family: &str) -> bool {
    matches!(
        (prefix, family),
        ("windows.native.prefetch.", "Prefetch")
            | ("windows.native.activity.", "LNK" | "JumpList")
            | ("windows.native.recycle_bin.", "RecycleBin")
            | ("windows.native.thumbcache.", "Thumbcache")
            | ("windows.native.sru.", "SRU")
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/replacement_index.rs"]
mod tests;
