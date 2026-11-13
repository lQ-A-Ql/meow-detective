use persistence_source::fjall::instrumentation;
use search::FileSearchSortDirection;
use std::{cmp::Ordering, collections::BinaryHeap};

use super::search_rank_order;

struct Head {
    source: usize,
    sort_value: String,
    file_id: String,
    direction: FileSearchSortDirection,
}

impl PartialEq for Head {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Head {}

impl PartialOrd for Head {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Head {
    fn cmp(&self, other: &Self) -> Ordering {
        instrumentation::record_merge_comparison();
        search_rank_order(
            &other.sort_value,
            &other.file_id,
            &self.sort_value,
            &self.file_id,
            self.direction,
        )
        .then_with(|| other.source.cmp(&self.source))
    }
}

pub(super) struct MergeHeap {
    heads: BinaryHeap<Head>,
    direction: FileSearchSortDirection,
}

impl MergeHeap {
    pub(super) fn new(direction: FileSearchSortDirection) -> Self {
        Self {
            heads: BinaryHeap::new(),
            direction,
        }
    }

    pub(super) fn push(&mut self, source: usize, sort_value: &str, file_id: &str) {
        self.heads.push(Head {
            source,
            sort_value: sort_value.to_owned(),
            file_id: file_id.to_owned(),
            direction: self.direction,
        });
    }

    pub(super) fn pop(&mut self) -> Option<usize> {
        self.heads.pop().map(|head| head.source)
    }
}
