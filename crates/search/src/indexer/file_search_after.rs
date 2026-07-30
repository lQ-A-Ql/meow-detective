use std::cmp::Ordering;

use tantivy::collector::sort_key::Comparator;
use tantivy::collector::{Collector, SegmentCollector, TopNComputer};
use tantivy::columnar::StrColumn;
use tantivy::{DocAddress, DocId, SegmentOrdinal, SegmentReader};

use super::file_query::FileSearchSortDirection;
use super::file_query_session::FileSearchAfterKey;

pub(super) struct FileSearchAfterCollector<'a> {
    limit: usize,
    after: Option<&'a FileSearchAfterKey>,
    sort_field: &'a str,
    direction: FileSearchSortDirection,
}

impl<'a> FileSearchAfterCollector<'a> {
    pub(super) fn new(
        limit: usize,
        after: Option<&'a FileSearchAfterKey>,
        sort_field: &'a str,
        direction: FileSearchSortDirection,
    ) -> Self {
        Self {
            limit,
            after,
            sort_field,
            direction,
        }
    }
}

pub(super) struct FileSearchCandidate {
    pub(super) file_id: String,
    pub(super) sort_value: String,
    pub(super) address: DocAddress,
}

#[derive(Clone)]
struct SegmentKey {
    sort_ord: u64,
    file_id_ord: u64,
}

#[derive(Clone)]
struct SegmentAfterBound {
    sort_ord: u64,
    sort_exact: bool,
    first_file_id_after: u64,
}

#[derive(Debug, Clone)]
struct FileRankComparator {
    direction: FileSearchSortDirection,
}

impl Default for FileRankComparator {
    fn default() -> Self {
        Self {
            direction: FileSearchSortDirection::Asc,
        }
    }
}

impl Comparator<SegmentKey> for FileRankComparator {
    fn compare(&self, left: &SegmentKey, right: &SegmentKey) -> Ordering {
        let primary = match self.direction {
            FileSearchSortDirection::Asc => right.sort_ord.cmp(&left.sort_ord),
            FileSearchSortDirection::Desc => left.sort_ord.cmp(&right.sort_ord),
        };
        primary.then_with(|| right.file_id_ord.cmp(&left.file_id_ord))
    }
}

pub(super) struct FileSearchSegmentCollector {
    segment_ord: SegmentOrdinal,
    sort_values: StrColumn,
    file_ids: StrColumn,
    after: Option<SegmentAfterBound>,
    direction: FileSearchSortDirection,
    top_docs: TopNComputer<SegmentKey, DocId, FileRankComparator>,
    invalid_document: bool,
}

impl Collector for FileSearchAfterCollector<'_> {
    type Fruit = Vec<FileSearchCandidate>;
    type Child = FileSearchSegmentCollector;

    fn for_segment(
        &self,
        segment_ord: SegmentOrdinal,
        segment: &SegmentReader,
    ) -> tantivy::Result<Self::Child> {
        let sort_values = required_column(segment, self.sort_field)?;
        let file_ids = required_column(segment, "file_id")?;
        let after = self
            .after
            .map(|key| segment_after_bound(&sort_values, &file_ids, key))
            .transpose()?;
        Ok(FileSearchSegmentCollector {
            segment_ord,
            sort_values,
            file_ids,
            after,
            direction: self.direction,
            top_docs: TopNComputer::new_with_comparator(
                self.limit,
                FileRankComparator {
                    direction: self.direction,
                },
            ),
            invalid_document: false,
        })
    }

    fn requires_scoring(&self) -> bool {
        false
    }

    fn merge_fruits(
        &self,
        segment_fruits: Vec<<Self::Child as SegmentCollector>::Fruit>,
    ) -> tantivy::Result<Self::Fruit> {
        let mut candidates = Vec::new();
        for segment in segment_fruits {
            candidates.extend(segment?);
        }
        candidates.sort_unstable_by(|left, right| {
            file_rank_order(
                &left.sort_value,
                &left.file_id,
                &right.sort_value,
                &right.file_id,
                self.direction,
            )
        });
        candidates.truncate(self.limit);
        Ok(candidates)
    }
}

impl SegmentCollector for FileSearchSegmentCollector {
    type Fruit = tantivy::Result<Vec<FileSearchCandidate>>;

    fn collect(&mut self, doc: DocId, _score: tantivy::Score) {
        let Some(sort_ord) = single_ord(&self.sort_values, doc) else {
            self.invalid_document = true;
            return;
        };
        let Some(file_id_ord) = single_ord(&self.file_ids, doc) else {
            self.invalid_document = true;
            return;
        };
        if !self.is_after(sort_ord, file_id_ord) {
            return;
        }
        self.top_docs.push(
            SegmentKey {
                sort_ord,
                file_id_ord,
            },
            doc,
        );
    }

    fn harvest(self) -> Self::Fruit {
        if self.invalid_document {
            return Err(invalid_sort_document());
        }
        self.top_docs
            .into_sorted_vec()
            .into_iter()
            .map(|ranked| {
                Ok(FileSearchCandidate {
                    file_id: term_text(&self.file_ids, ranked.sort_key.file_id_ord)?,
                    sort_value: term_text(&self.sort_values, ranked.sort_key.sort_ord)?,
                    address: DocAddress::new(self.segment_ord, ranked.doc),
                })
            })
            .collect()
    }
}

impl FileSearchSegmentCollector {
    fn is_after(&self, sort_ord: u64, file_id_ord: u64) -> bool {
        let Some(after) = self.after.as_ref() else {
            return true;
        };
        if !after.sort_exact {
            return match self.direction {
                FileSearchSortDirection::Asc => sort_ord >= after.sort_ord,
                FileSearchSortDirection::Desc => sort_ord < after.sort_ord,
            };
        }
        match self.direction {
            FileSearchSortDirection::Asc if sort_ord > after.sort_ord => true,
            FileSearchSortDirection::Desc if sort_ord < after.sort_ord => true,
            _ if sort_ord == after.sort_ord => file_id_ord >= after.first_file_id_after,
            _ => false,
        }
    }
}

pub(super) fn file_rank_order(
    left_sort: &str,
    left_id: &str,
    right_sort: &str,
    right_id: &str,
    direction: FileSearchSortDirection,
) -> Ordering {
    let primary = match direction {
        FileSearchSortDirection::Asc => left_sort.cmp(right_sort),
        FileSearchSortDirection::Desc => right_sort.cmp(left_sort),
    };
    primary.then_with(|| left_id.cmp(right_id))
}

fn required_column(segment: &SegmentReader, field: &str) -> tantivy::Result<StrColumn> {
    segment.fast_fields().str(field)?.ok_or_else(|| {
        tantivy::TantivyError::SchemaError(format!("{field} must be a string fast field"))
    })
}

fn single_ord(column: &StrColumn, doc: DocId) -> Option<u64> {
    let mut ordinals = column.term_ords(doc);
    let value = ordinals.next()?;
    ordinals.next().is_none().then_some(value)
}

fn term_text(column: &StrColumn, ordinal: u64) -> tantivy::Result<String> {
    let mut bytes = Vec::new();
    column
        .dictionary()
        .ord_to_term(ordinal, &mut bytes)
        .map_err(|_| invalid_sort_document())?;
    String::from_utf8(bytes).map_err(|_| invalid_sort_document())
}

fn segment_after_bound(
    sort_values: &StrColumn,
    file_ids: &StrColumn,
    after: &FileSearchAfterKey,
) -> tantivy::Result<SegmentAfterBound> {
    let (sort_ord, sort_exact) = lower_bound(sort_values, after.sort_value.as_bytes())?;
    let (file_id_ord, file_id_exact) = lower_bound(file_ids, after.file_id.as_bytes())?;
    Ok(SegmentAfterBound {
        sort_ord,
        sort_exact,
        first_file_id_after: file_id_ord.saturating_add(u64::from(file_id_exact)),
    })
}

fn lower_bound(column: &StrColumn, target: &[u8]) -> tantivy::Result<(u64, bool)> {
    let mut low = 0u64;
    let mut high = column.dictionary().num_terms() as u64;
    let mut scratch = Vec::new();
    while low < high {
        let middle = low + (high - low) / 2;
        scratch.clear();
        let found = column
            .dictionary()
            .ord_to_term(middle, &mut scratch)
            .map_err(|_| invalid_sort_document())?;
        if !found {
            return Err(invalid_sort_document());
        }
        if scratch.as_slice() < target {
            low = middle.saturating_add(1);
        } else {
            high = middle;
        }
    }
    if low == column.dictionary().num_terms() as u64 {
        return Ok((low, false));
    }
    scratch.clear();
    let found = column
        .dictionary()
        .ord_to_term(low, &mut scratch)
        .map_err(|_| invalid_sort_document())?;
    if !found {
        return Err(invalid_sort_document());
    }
    Ok((low, scratch == target))
}

fn invalid_sort_document() -> tantivy::TantivyError {
    tantivy::TantivyError::SchemaError(
        "every file search document must have one UTF-8 sort value and file_id".to_string(),
    )
}
