use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

use super::ceph_rocksdb_spool::SpoolRange;

pub(super) struct RangeCoverage {
    ranges: Vec<SpoolRange>,
    next: usize,
    active: HashSet<usize>,
    by_end: BinaryHeap<Reverse<(Vec<u8>, usize)>>,
    by_sequence: BinaryHeap<(u64, usize)>,
}

impl RangeCoverage {
    pub(super) fn new(ranges: Vec<SpoolRange>) -> Self {
        Self {
            ranges,
            next: 0,
            active: HashSet::new(),
            by_end: BinaryHeap::new(),
            by_sequence: BinaryHeap::new(),
        }
    }

    pub(super) fn covering_sequence(&mut self, user_key: &[u8]) -> Option<u64> {
        self.activate_started_ranges(user_key);
        self.expire_finished_ranges(user_key);
        self.highest_active_sequence()
    }

    fn activate_started_ranges(&mut self, user_key: &[u8]) {
        while self
            .ranges
            .get(self.next)
            .is_some_and(|range| range.start_key.as_slice() <= user_key)
        {
            let index = self.next;
            let range = &self.ranges[index];
            self.active.insert(index);
            self.by_end.push(Reverse((range.end_key.clone(), index)));
            self.by_sequence.push((range.sequence, index));
            self.next += 1;
        }
    }

    fn expire_finished_ranges(&mut self, user_key: &[u8]) {
        while self
            .by_end
            .peek()
            .is_some_and(|Reverse((end_key, _))| end_key.as_slice() <= user_key)
        {
            if let Some(Reverse((_, index))) = self.by_end.pop() {
                self.active.remove(&index);
            }
        }
    }

    fn highest_active_sequence(&mut self) -> Option<u64> {
        while self
            .by_sequence
            .peek()
            .is_some_and(|(_, index)| !self.active.contains(index))
        {
            self.by_sequence.pop();
        }
        self.by_sequence.peek().map(|(sequence, _)| *sequence)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_range_state.rs"]
mod tests;
