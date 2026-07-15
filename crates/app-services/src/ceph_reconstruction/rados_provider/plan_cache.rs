use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::super::rados_reader::RadosObjectLayout;

const MAX_PLAN_BYTES: usize = 16 * 1024 * 1024;
const MAX_PLANS: usize = 128;

pub(super) struct ObjectPlanCache {
    entries: HashMap<String, Arc<RadosObjectLayout>>,
    lru: VecDeque<String>,
    total_bytes: usize,
    max_entries: usize,
    max_bytes: usize,
}

impl ObjectPlanCache {
    pub(super) fn for_rbd() -> Self {
        Self {
            entries: HashMap::new(),
            lru: VecDeque::new(),
            total_bytes: 0,
            max_entries: MAX_PLANS,
            max_bytes: MAX_PLAN_BYTES,
        }
    }

    pub(super) fn get(&mut self, object_identity: &str) -> Option<Arc<RadosObjectLayout>> {
        let plan = self.entries.get(object_identity)?.clone();
        self.touch(object_identity);
        Some(plan)
    }

    pub(super) fn insert(&mut self, object_identity: String, plan: Arc<RadosObjectLayout>) {
        let plan_bytes = plan.estimated_bytes();
        if plan_bytes > self.max_bytes {
            return;
        }
        if let Some(previous) = self.entries.remove(&object_identity) {
            self.total_bytes = self.total_bytes.saturating_sub(previous.estimated_bytes());
        }
        self.entries.insert(object_identity.clone(), plan);
        self.lru.retain(|entry| entry != &object_identity);
        self.lru.push_back(object_identity);
        self.total_bytes = self.total_bytes.saturating_add(plan_bytes);
        while self.entries.len() > self.max_entries || self.total_bytes > self.max_bytes {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.estimated_bytes());
            }
        }
    }

    fn touch(&mut self, object_identity: &str) {
        self.lru.retain(|entry| entry != object_identity);
        self.lru.push_back(object_identity.to_string());
    }
}
