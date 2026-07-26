use std::cmp::Ordering;

use tantivy::collector::sort_key::Comparator;
use tantivy::collector::{Collector, SegmentCollector, TopNComputer};
use tantivy::columnar::StrColumn;
use tantivy::{DocAddress, DocId, Score, SegmentOrdinal, SegmentReader};

use super::query_session::SearchAfterKey;

#[derive(Debug)]
pub(super) struct SearchAfterCollector<'a> {
    limit: usize,
    after: Option<&'a SearchAfterKey>,
}

impl<'a> SearchAfterCollector<'a> {
    pub(super) fn new(limit: usize, after: Option<&'a SearchAfterKey>) -> Self {
        Self { limit, after }
    }
}

#[derive(Debug)]
pub(super) struct SearchAfterCandidate {
    pub(super) file_id: String,
    pub(super) score: Score,
    pub(super) address: DocAddress,
}

#[derive(Debug, Clone)]
struct SegmentRankKey {
    score: Score,
    file_id_ord: u64,
}

#[derive(Debug, Default)]
struct SearchRankComparator;

impl Comparator<SegmentRankKey> for SearchRankComparator {
    fn compare(&self, left: &SegmentRankKey, right: &SegmentRankKey) -> Ordering {
        left.score
            .total_cmp(&right.score)
            .then_with(|| right.file_id_ord.cmp(&left.file_id_ord))
    }
}

pub(super) struct SearchAfterSegmentCollector {
    segment_ord: SegmentOrdinal,
    file_ids: StrColumn,
    after: Option<SearchAfterKey>,
    top_docs: TopNComputer<SegmentRankKey, DocId, SearchRankComparator>,
    invalid_document: bool,
    scratch: Vec<u8>,
}

impl Collector for SearchAfterCollector<'_> {
    type Fruit = Vec<SearchAfterCandidate>;
    type Child = SearchAfterSegmentCollector;

    fn for_segment(
        &self,
        segment_ord: SegmentOrdinal,
        segment: &SegmentReader,
    ) -> tantivy::Result<Self::Child> {
        let file_ids = segment.fast_fields().str("file_id")?.ok_or_else(|| {
            tantivy::TantivyError::SchemaError(
                "file_id must be available as a string fast field".to_string(),
            )
        })?;
        Ok(SearchAfterSegmentCollector {
            segment_ord,
            file_ids,
            after: self.after.cloned(),
            top_docs: TopNComputer::new_with_comparator(self.limit, SearchRankComparator),
            invalid_document: false,
            scratch: Vec::new(),
        })
    }

    fn requires_scoring(&self) -> bool {
        true
    }

    fn merge_fruits(
        &self,
        segment_fruits: Vec<<Self::Child as SegmentCollector>::Fruit>,
    ) -> tantivy::Result<Self::Fruit> {
        let mut candidates = Vec::new();
        for segment in segment_fruits {
            candidates.extend(segment?);
        }
        candidates.sort_unstable_by(candidate_order);
        candidates.truncate(self.limit);
        Ok(candidates)
    }
}

impl SegmentCollector for SearchAfterSegmentCollector {
    type Fruit = tantivy::Result<Vec<SearchAfterCandidate>>;

    fn collect(&mut self, doc: DocId, score: Score) {
        let mut ordinals = self.file_ids.term_ords(doc);
        let Some(file_id_ord) = ordinals.next() else {
            self.invalid_document = true;
            return;
        };
        let has_multiple_values = ordinals.next().is_some();
        drop(ordinals);
        if has_multiple_values {
            self.invalid_document = true;
            return;
        }
        if !self.is_after_cursor(score, file_id_ord) {
            return;
        }
        self.top_docs
            .push(SegmentRankKey { score, file_id_ord }, doc);
    }

    fn harvest(self) -> Self::Fruit {
        if self.invalid_document {
            return Err(invalid_file_id_document());
        }
        let mut candidates = Vec::new();
        for ranked in self.top_docs.into_sorted_vec() {
            let mut bytes = Vec::new();
            self.file_ids
                .dictionary()
                .ord_to_term(ranked.sort_key.file_id_ord, &mut bytes)
                .map_err(|_| invalid_file_id_document())?;
            let file_id = String::from_utf8(bytes).map_err(|_| invalid_file_id_document())?;
            candidates.push(SearchAfterCandidate {
                file_id,
                score: ranked.sort_key.score,
                address: DocAddress::new(self.segment_ord, ranked.doc),
            });
        }
        Ok(candidates)
    }
}

fn invalid_file_id_document() -> tantivy::TantivyError {
    tantivy::TantivyError::SchemaError(
        "every searchable document must contain exactly one UTF-8 file_id value".to_string(),
    )
}

impl SearchAfterSegmentCollector {
    fn is_after_cursor(&mut self, score: Score, file_id_ord: u64) -> bool {
        let Some(after) = self.after.as_ref() else {
            return true;
        };
        let cursor_score = Score::from_bits(after.score_bits);
        match cursor_score.total_cmp(&score) {
            Ordering::Less => false,
            Ordering::Greater => true,
            Ordering::Equal => {
                self.scratch.clear();
                if self
                    .file_ids
                    .dictionary()
                    .ord_to_term(file_id_ord, &mut self.scratch)
                    .is_err()
                {
                    self.invalid_document = true;
                    return false;
                }
                self.scratch.as_slice() > after.file_id.as_bytes()
            }
        }
    }
}

fn candidate_order(left: &SearchAfterCandidate, right: &SearchAfterCandidate) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.file_id.cmp(&right.file_id))
}
