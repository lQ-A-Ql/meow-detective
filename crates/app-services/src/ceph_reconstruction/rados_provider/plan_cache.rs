use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::super::rados_reader::RadosObjectLayout;

pub(super) const MAX_PLAN_BYTES: usize = 16 * 1024 * 1024;
const MAX_PLANS: usize = 128;
const CACHE_QUEUE_COMPACTION_FACTOR: usize = 4;

struct CachedPlan {
    layout: Arc<RadosObjectLayout>,
    generation: u64,
}

pub(super) struct ObjectPlanCache {
    entries: HashMap<String, CachedPlan>,
    access_order: VecDeque<(String, u64)>,
    total_bytes: usize,
    max_entries: usize,
    max_bytes: usize,
    generation: u64,
}

impl ObjectPlanCache {
    pub(super) fn for_rbd() -> Self {
        Self {
            entries: HashMap::new(),
            access_order: VecDeque::new(),
            total_bytes: 0,
            max_entries: MAX_PLANS,
            max_bytes: MAX_PLAN_BYTES,
            generation: 0,
        }
    }

    pub(super) fn get(&mut self, object_identity: &str) -> Option<Arc<RadosObjectLayout>> {
        let generation = self.next_generation();
        let layout = {
            let cached = self.entries.get_mut(object_identity)?;
            cached.generation = generation;
            Arc::clone(&cached.layout)
        };
        self.access_order
            .push_back((object_identity.to_string(), generation));
        self.compact_access_order_if_needed();
        Some(layout)
    }

    pub(super) fn insert(&mut self, object_identity: String, plan: Arc<RadosObjectLayout>) {
        let plan_bytes = plan.estimated_bytes();
        if plan_bytes > self.max_bytes {
            return;
        }
        if let Some(previous) = self.entries.remove(&object_identity) {
            self.total_bytes = self
                .total_bytes
                .saturating_sub(previous.layout.estimated_bytes());
        }
        self.total_bytes = self.total_bytes.saturating_add(plan_bytes);
        let generation = self.next_generation();
        self.access_order
            .push_back((object_identity.clone(), generation));
        self.entries.insert(
            object_identity,
            CachedPlan {
                layout: plan,
                generation,
            },
        );
        self.evict();
        self.compact_access_order_if_needed();
    }

    fn evict(&mut self) {
        while self.entries.len() > self.max_entries || self.total_bytes > self.max_bytes {
            let Some((oldest, generation)) = self.access_order.pop_front() else {
                break;
            };
            let is_current = self
                .entries
                .get(&oldest)
                .is_some_and(|cached| cached.generation == generation);
            if !is_current {
                continue;
            }
            if let Some(removed) = self.entries.remove(&oldest) {
                self.total_bytes = self
                    .total_bytes
                    .saturating_sub(removed.layout.estimated_bytes());
            }
        }
    }

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }

    fn compact_access_order_if_needed(&mut self) {
        let limit = self
            .entries
            .len()
            .saturating_mul(CACHE_QUEUE_COMPACTION_FACTOR)
            .max(128);
        if self.access_order.len() <= limit {
            return;
        }
        let mut current = self
            .entries
            .iter()
            .map(|(identity, cached)| (cached.generation, identity.clone()))
            .collect::<Vec<_>>();
        current.sort_by_key(|(generation, _)| *generation);
        self.access_order = current
            .into_iter()
            .map(|(generation, identity)| (identity, generation))
            .collect();
    }
}
